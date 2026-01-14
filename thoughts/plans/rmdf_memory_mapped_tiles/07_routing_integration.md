# Phase 7: Routing Integration

## Overview

Integrate TileManager into the routing system, replacing all MapDataGraph::get() calls. Update walker, navigator, and router_runner to use TileManager with tile-aware references.

**Goals:**
- Replace MapDataGraph::get() with TileManager in routing code
- Update reference types to include tile_id
- Update CLI to accept --tiles directory
- Maintain routing algorithm logic unchanged
- End-to-end routing test with Montenegro tiles

## Changes Required

### 1. Update Router Runner

**File**: `src/router_runner.rs`

**Changes**: Initialize TileManager instead of MapDataGraph

```rust
impl RouterRunner {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            RouterCommand::GenerateRoute {
                input,
                cache_dir,  // DEPRECATED - will be removed in Phase 8
                tiles,      // NEW parameter
                output,
                rule_file,
                debug_dir,
                routing_mode,
            } => {
                // OLD CODE (to be removed in Phase 8):
                // let mut data_cache = MapDataCache::init(cache_dir, &data_source);
                // MapDataGraph::init(&data_source);

                // NEW CODE:
                let tiles_dir = tiles.ok_or_else(|| {
                    anyhow::anyhow!("--tiles directory required")
                })?;

                let mut tile_manager = crate::rmdf::TileManager::new(tiles_dir)?;

                // Generate route using TileManager
                let generator = self.create_generator(&mut tile_manager, &routing_mode, &rule_file)?;
                let routes = generator.generate_routes()?;

                // Output routes (unchanged)
                self.write_output(output, routes)?;

                Ok(())
            },
            // ... other commands
        }
    }

    fn create_generator(
        &self,
        tile_manager: &mut crate::rmdf::TileManager,
        routing_mode: &RoutingMode,
        rule_file: &Option<PathBuf>,
    ) -> Result<Generator> {
        let rules = self.load_rules(rule_file)?;

        let (start, finish) = match routing_mode {
            RoutingMode::StartFinish { start, finish } => {
                let start_point = tile_manager.get_closest_to_coords(
                    start.0,
                    start.1,
                    // ... rules, avoid_proximity, limit_to_hw_tags
                )?;

                let finish_point = tile_manager.get_closest_to_coords(
                    finish.0,
                    finish.1,
                    // ... rules, avoid_proximity, limit_to_hw_tags
                )?;

                (start_point, finish_point)
            },
            // ... other modes
        };

        Ok(Generator::new(start?, finish?, /*...*/ ))
    }
}
```

**Rationale**: TileManager replaces MapDataGraph as data source.

### 2. Update CLI Arguments

**File**: `src/router_runner.rs`

**Changes**: Add --tiles parameter to GenerateRoute

```rust
RouterCommand::GenerateRoute {
    /// Input file (DEPRECATED - use --tiles instead)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Cache directory (DEPRECATED - will be removed)
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Tiles directory (NEW - required)
    #[arg(long)]
    tiles: Option<PathBuf>,

    // ... existing output, rule-file, debug-dir, routing-mode
}
```

**Rationale**: Maintains backward compatibility temporarily (Phase 8 removes deprecated options).

### 3. Update Walker

**File**: `src/router/walker.rs`

**Changes**: Replace MapDataGraph::get() with TileManager

```rust
use crate::rmdf::{TileManager, PointRef, LineRef};

pub struct Walker {
    tile_manager: &'static mut TileManager,  // Replace MapDataGraph reference
    current_point: PointRef,                 // Now includes tile_id
    route: Vec<RouteSegment>,
    visited_junctions: HashSet<u64>,         // OSM IDs (not tile-aware)
}

impl Walker {
    pub fn new(tile_manager: &'static mut TileManager, start_point: PointRef) -> Self {
        Self {
            tile_manager,
            current_point: start_point,
            route: Vec::new(),
            visited_junctions: HashSet::new(),
        }
    }

    // Line 58: OLD: MapDataGraph::get().get_adjacent(center_point.clone())
    // Line 58: NEW:
    fn get_segments_for_point(&mut self, center_point: &PointRef) -> Vec<(LineRef, PointRef)> {
        self.tile_manager.get_adjacent(center_point)
            .unwrap_or_else(|e| {
                tracing::error!("Failed to get adjacent: {}", e);
                Vec::new()
            })
    }

    // Update all methods to use PointRef instead of MapDataPointRef
    // Update all .borrow() calls to use TileManager methods
}
```

**Rationale**:
- Same algorithm, different data source
- PointRef includes tile context
- Error handling for missing tiles

### 4. Update Navigator

**File**: `src/router/navigator.rs`

**Changes**: Similar TileManager integration

```rust
use crate::rmdf::{TileManager, PointRef};

pub struct Navigator {
    tile_manager: &'static mut TileManager,
    walker: Walker,
    itinerary: Itinerary,
    // ... other fields
}

impl Navigator {
    pub fn new(
        tile_manager: &'static mut TileManager,
        start: PointRef,
        finish: PointRef,
        // ... other params
    ) -> Self {
        let walker = Walker::new(tile_manager, start);
        // ...
    }

    // Line 221: OLD: self.walker.get_last_point()
    // Line 221: NEW: (same - PointRef returned)

    // All routing logic unchanged - just reference types different
}
```

**Rationale**: Minimal changes to routing algorithm.

### 5. Update Generator

**File**: `src/router/generator.rs`

**Changes**: Pass TileManager to Navigator

```rust
pub struct Generator {
    tile_manager: &'static mut TileManager,
    start: PointRef,
    finish: PointRef,
    // ... other fields
}

impl Generator {
    pub fn generate_routes(&mut self) -> Result<Vec<Route>> {
        // Create navigators with TileManager
        let itineraries = self.create_itineraries()?;

        let routes = itineraries.into_par_iter()
            .map(|itinerary| {
                let navigator = Navigator::new(
                    self.tile_manager,
                    self.start.clone(),
                    self.finish.clone(),
                    // ...
                );
                navigator.generate_routes()
            })
            .collect();

        Ok(routes)
    }
}
```

**Rationale**: Thread TileManager through routing pipeline.

### 6. Handle Static Lifetime Requirement

**Note**: The current codebase uses `&'static` references from MapDataGraph. TileManager cannot provide static references. Two options:

**Option A**: Change lifetimes throughout routing code (invasive)
**Option B**: Use unsafe to extend TileManager lifetime (pragmatic)

**Recommendation**: Option B for Phase 7, refactor to Option A in future.

**File**: `src/router_runner.rs`

**Changes**: Extend TileManager lifetime

```rust
// SAFETY: TileManager lives for entire routing request
// This is safe because we control the execution flow
let tile_manager_static: &'static mut TileManager = unsafe {
    std::mem::transmute(&mut tile_manager)
};

let generator = Generator::new(tile_manager_static, /*...*/);
```

**Rationale**: Pragmatic solution; routing code unchanged. Can refactor lifetimes later.

## Success Criteria

### Automated Verification

- [ ] Routing tests pass: `cargo test router`
- [ ] Type checking passes: `cargo check`
- [ ] No compiler warnings: `cargo clippy`

### Manual Verification

- [ ] Generate Montenegro tiles (from Phase 4)
- [ ] Route between two coordinates: `ridi-router generate-route --tiles ./tiles --routing-mode start-finish --start 42.5,18.5 --finish 42.6,18.6 --output route.gpx`
- [ ] Route generated successfully (GPX or JSON output)
- [ ] Route crosses tile boundaries (verify with debug logging)
- [ ] Route sensible (not obviously broken)
- [ ] Performance comparable to old system

## Dependencies

- **Depends on**: Phase 6 (needs get_adjacent() with border crossing)
- **Blocks**: Phase 8 (cleanup requires working routing)

## Risks & Mitigations

**Risk**: Lifetime issues with TileManager
- **Mitigation**: Use unsafe transmute temporarily, document for future refactoring

**Risk**: Performance regression from tile loading
- **Mitigation**: Acceptable for MVP; optimize later with caching/prefetching

**Risk**: Routing algorithm breaks with new references
- **Mitigation**: Extensive testing, compare routes with old system

## Notes

- Routing algorithm logic unchanged (only data source changed)
- PointRef/LineRef include tile_id but routing doesn't care
- Missing tiles handled gracefully (route around or fail)
- --tiles parameter required, --input/--cache-dir deprecated (removed in Phase 8)
- Static lifetime workaround documented for future cleanup
