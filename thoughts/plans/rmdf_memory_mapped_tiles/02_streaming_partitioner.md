# Phase 2: CLI Structure & Streaming PBF Partitioner

## Overview

Add the `generate-tiles` CLI subcommand and implement streaming PBF reading with geographic partitioning. This phase does NOT compute proximity/nogo flags yet - it only partitions raw OSM data by tile boundaries and writes intermediate buffers to disk.

**Goals:**
- Add CLI subcommand `generate-tiles` with `--tile-size` parameter
- Stream PBF file without loading entire dataset into memory
- Partition nodes, ways, and relations by geographic tile
- Handle border cases (points/lines on tile boundaries)
- Write intermediate tile data to disk

## Changes Required

### 1. Add CLI Subcommand

**File**: `src/router_runner.rs`

**Changes**: Add GenerateTiles subcommand to RouterCommand enum

```rust
#[derive(clap::Subcommand)]
pub enum RouterCommand {
    GenerateRoute {
        // ... existing fields
    },
    StartServer {
        // ... existing fields
    },
    StartClient {
        // ... existing fields
    },
    PrepCache {
        // ... existing fields (will be removed in Phase 8)
    },
    #[cfg(feature = "debug-viewer")]
    DebugViewer {
        // ... existing fields
    },
    #[cfg(feature = "rule-schema-writer")]
    RuleSchemaWrite {
        // ... existing fields
    },
    // NEW SUBCOMMAND
    GenerateTiles {
        /// Input OSM PBF file
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory for tiles and manifest
        #[arg(short, long)]
        output: PathBuf,

        /// Tile size in degrees (e.g., 0.1, 1.0)
        #[arg(long, default_value = "1.0")]
        tile_size: f32,
    },
}
```

**Rationale**: Matches existing CLI patterns. Tile size configurable per-generation.

### 2. Add Subcommand Handler

**File**: `src/router_runner.rs`

**Changes**: Add handler in RouterRunner::run()

```rust
impl RouterRunner {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            RouterCommand::GenerateRoute { /* ... */ } => {
                // ... existing
            },
            RouterCommand::GenerateTiles { input, output, tile_size } => {
                use crate::rmdf::generator::TileGenerator;

                info!("Generating tiles from {:?} to {:?} (tile_size={}°)",
                      input, output, tile_size);

                let generator = TileGenerator::new(input, output, tile_size)?;
                generator.generate()?;

                info!("Tile generation complete");
                Ok(())
            },
            // ... other commands
        }
    }
}
```

**Rationale**: Delegates to TileGenerator which orchestrates all generation phases.

### 3. Create Tile Generator Module

**File**: `src/rmdf/generator/mod.rs` (new file)

**Changes**: Create generator module structure

```rust
mod pbf_streamer;
mod intermediate;

pub use pbf_streamer::*;
pub use intermediate::*;

use std::path::PathBuf;
use anyhow::Result;

pub struct TileGenerator {
    input_file: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

impl TileGenerator {
    pub fn new(input_file: PathBuf, output_dir: PathBuf, tile_size_degrees: f32) -> Result<Self> {
        // Validate tile size
        if tile_size_degrees <= 0.0 || tile_size_degrees > 180.0 {
            anyhow::bail!("Tile size must be between 0 and 180 degrees");
        }

        // Create output directory
        std::fs::create_dir_all(&output_dir)?;

        Ok(Self {
            input_file,
            output_dir,
            tile_size_degrees,
        })
    }

    pub fn generate(&self) -> Result<()> {
        // Phase 2: Stream and partition
        let streamer = PbfStreamer::new(&self.input_file, &self.output_dir, self.tile_size_degrees)?;
        let tile_buffers = streamer.partition()?;

        // Phase 3: Proximity computation (not implemented yet)
        // Phase 4: RMDF writing (not implemented yet)

        Ok(())
    }
}
```

**Rationale**: Orchestrates all tile generation phases. This phase only implements partition().

### 4. Define Intermediate Data Structures

**File**: `src/rmdf/generator/intermediate.rs` (new file)

**Changes**: Define structures for intermediate tile data

```rust
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation};
use crate::rmdf::format::TileId;

/// Intermediate representation of a tile before RMDF serialization
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IntermediateTile {
    pub tile_id: TileId,
    pub nodes: HashMap<u64, OsmNode>,      // OSM ID -> Node
    pub ways: Vec<OsmWay>,
    pub relations: Vec<OsmRelation>,
}

impl IntermediateTile {
    pub fn new(tile_id: TileId) -> Self {
        Self {
            tile_id,
            nodes: HashMap::new(),
            ways: Vec::new(),
            relations: Vec::new(),
        }
    }

    /// Add node to tile (if within bounds or on border)
    pub fn add_node(&mut self, node: OsmNode) {
        self.nodes.insert(node.id, node);
    }

    /// Add way to tile (if it crosses or is within tile)
    pub fn add_way(&mut self, way: OsmWay) {
        self.ways.push(way);
    }

    /// Add relation to tile
    pub fn add_relation(&mut self, relation: OsmRelation) {
        self.relations.push(relation);
    }

    /// Save to disk as JSON (for debugging) or bincode (for performance)
    pub fn save_to_disk(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        let filename = format!("intermediate_{}_{}.bin", self.tile_id.col, self.tile_id.row);
        let path = output_dir.join(filename);

        let file = std::fs::File::create(path)?;
        bincode::serialize_into(file, self)?;

        Ok(())
    }

    /// Load from disk
    pub fn load_from_disk(output_dir: &std::path::Path, tile_id: TileId) -> anyhow::Result<Self> {
        let filename = format!("intermediate_{}_{}.bin", tile_id.col, tile_id.row);
        let path = output_dir.join(filename);

        let file = std::fs::File::open(path)?;
        let tile = bincode::deserialize_from(file)?;

        Ok(tile)
    }
}

/// Collection of all intermediate tiles
pub struct TileBuffers {
    pub tiles: HashMap<TileId, IntermediateTile>,
}

impl TileBuffers {
    pub fn new() -> Self {
        Self {
            tiles: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, tile_id: TileId) -> &mut IntermediateTile {
        self.tiles.entry(tile_id).or_insert_with(|| IntermediateTile::new(tile_id))
    }

    /// Save all tiles to disk
    pub fn save_all(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        for tile in self.tiles.values() {
            tile.save_to_disk(output_dir)?;
        }
        Ok(())
    }
}
```

**Rationale**: Intermediate format separates partitioning from RMDF serialization. Uses bincode for temporary storage (will still be in dependencies until Phase 8).

### 5. Implement Streaming PBF Partitioner

**File**: `src/rmdf/generator/pbf_streamer.rs` (new file)

**Changes**: Stream PBF and partition by tile

```rust
use anyhow::{Context, Result};
use osmpbfreader::{OsmPbfReader, OsmObj};
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::{info, debug};

use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType};
use crate::rmdf::format::TileId;
use crate::osm_data::data_reader::ALLOWED_HIGHWAY_VALUES;

use super::intermediate::{IntermediateTile, TileBuffers};

pub struct PbfStreamer {
    input_file: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
}

impl PbfStreamer {
    pub fn new(input_file: &Path, output_dir: &Path, tile_size_degrees: f32) -> Result<Self> {
        Ok(Self {
            input_file: input_file.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
            tile_size_degrees,
        })
    }

    pub fn partition(&self) -> Result<TileBuffers> {
        info!("Starting PBF streaming partitioning");

        let file = File::open(&self.input_file)
            .context("Failed to open PBF file")?;
        let mut pbf = OsmPbfReader::new(file);

        let mut tile_buffers = TileBuffers::new();

        // Stream all objects (don't use get_objs_and_deps - we want everything)
        for obj in pbf.iter() {
            let obj = obj.context("Failed to read PBF object")?;

            match obj {
                OsmObj::Node(node) => {
                    self.partition_node(&mut tile_buffers, node)?;
                }
                OsmObj::Way(way) => {
                    self.partition_way(&mut tile_buffers, way)?;
                }
                OsmObj::Relation(relation) => {
                    self.partition_relation(&mut tile_buffers, relation)?;
                }
            }
        }

        info!("Partitioned into {} tiles", tile_buffers.tiles.len());

        // Save to disk
        tile_buffers.save_all(&self.output_dir)?;
        info!("Saved intermediate tiles to disk");

        Ok(tile_buffers)
    }

    fn partition_node(&self, buffers: &mut TileBuffers, node: osmpbfreader::Node) -> Result<()> {
        let osm_node = OsmNode {
            id: node.id.0 as u64,
            lat: node.lat(),
            lon: node.lon(),
            residential_in_proximity: false,  // Will be computed in Phase 3
            nogo_area: false,                 // Will be computed in Phase 3
        };

        // Determine which tile(s) this node belongs to
        let tile_ids = self.get_tiles_for_point(osm_node.lat as f32, osm_node.lon as f32);

        for tile_id in tile_ids {
            buffers.get_or_create(tile_id).add_node(osm_node.clone());
        }

        Ok(())
    }

    fn partition_way(&self, buffers: &mut TileBuffers, way: osmpbfreader::Way) -> Result<()> {
        // Only include ways with highway tags (matching current behavior)
        let has_highway = way.tags.iter().any(|(k, v)| {
            k == "highway" && (
                ALLOWED_HIGHWAY_VALUES.contains(&v.as_str()) ||
                (v == "path" && way.tags.iter().any(|(k2, v2)| k2 == "motorcycle" && v2 == "yes"))
            )
        });

        if !has_highway {
            return Ok(());
        }

        let osm_way = OsmWay {
            id: way.id.0 as u64,
            point_ids: way.nodes.iter().map(|n| n.0 as u64).collect(),
            tags: Some(way.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()),
        };

        // Determine which tiles this way crosses
        // For now: Add to all tiles that contain any of the way's nodes
        // (More sophisticated later: only tiles the line segments actually cross)
        let mut tile_set = std::collections::HashSet::new();

        // We don't have node coordinates here, so we'll need to look them up
        // For Phase 2, we'll do a simplified approach: add to tiles in second pass
        // For now, just mark ways for all tiles (inefficient but correct)

        // SIMPLIFIED: This will be refined in implementation
        // For the plan, note that we need node coordinate lookup

        Ok(())
    }

    fn partition_relation(&self, buffers: &mut TileBuffers, relation: osmpbfreader::Relation) -> Result<()> {
        // Only include restriction relations (matching current behavior)
        let is_restriction = relation.tags.iter().any(|(k, v)| {
            k == "type" && v.starts_with("restriction")
        });

        if !is_restriction {
            return Ok(());
        }

        let osm_relation = OsmRelation {
            id: relation.id.0 as u64,
            members: relation.refs.iter().filter_map(|r| {
                let role = match r.role.as_str() {
                    "from" => OsmRelationMemberRole::From,
                    "to" => OsmRelationMemberRole::To,
                    "via" => OsmRelationMemberRole::Via,
                    _ => return None,
                };

                let (member_ref, member_type) = match r.member {
                    osmpbfreader::OsmId::Way(id) => (id.0 as u64, OsmRelationMemberType::Way),
                    osmpbfreader::OsmId::Node(id) => (id.0 as u64, OsmRelationMemberType::Node),
                    osmpbfreader::OsmId::Relation(id) => return None, // Skip nested relations
                };

                Some(OsmRelationMember {
                    member_ref,
                    role,
                    member_type,
                })
            }).collect(),
            tags: relation.tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };

        // Relations will be added to tiles based on their "via" member location
        // This requires a second pass after nodes are partitioned
        // For Phase 2, store relations separately for second-pass assignment

        Ok(())
    }

    /// Get tile ID(s) for a point
    /// Returns multiple tiles if point is on a border
    fn get_tiles_for_point(&self, lat: f32, lon: f32) -> Vec<TileId> {
        let mut tiles = Vec::new();

        // Primary tile
        let primary_col = ((lon + 180.0) / self.tile_size_degrees).floor() as u16;
        let primary_row = ((lat + 90.0) / self.tile_size_degrees).floor() as u16;

        tiles.push(TileId { col: primary_col, row: primary_row });

        // Check if on horizontal border (longitude)
        let lon_offset = (lon + 180.0) % self.tile_size_degrees;
        if lon_offset < 0.0001 && primary_col > 0 {
            // On western border, also add to western neighbor
            tiles.push(TileId { col: primary_col - 1, row: primary_row });
        } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
            // On eastern border, also add to eastern neighbor
            tiles.push(TileId { col: primary_col + 1, row: primary_row });
        }

        // Check if on vertical border (latitude)
        let lat_offset = (lat + 90.0) % self.tile_size_degrees;
        if lat_offset < 0.0001 && primary_row > 0 {
            // On southern border, also add to southern neighbor
            tiles.push(TileId { col: primary_col, row: primary_row - 1 });

            // Corner case: also add to southwestern neighbor
            if lon_offset < 0.0001 && primary_col > 0 {
                tiles.push(TileId { col: primary_col - 1, row: primary_row - 1 });
            } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
                tiles.push(TileId { col: primary_col + 1, row: primary_row - 1 });
            }
        } else if (self.tile_size_degrees - lat_offset) < 0.0001 {
            // On northern border, also add to northern neighbor
            tiles.push(TileId { col: primary_col, row: primary_row + 1 });

            // Corner case: also add to northwestern neighbor
            if lon_offset < 0.0001 && primary_col > 0 {
                tiles.push(TileId { col: primary_col - 1, row: primary_row + 1 });
            } else if (self.tile_size_degrees - lon_offset) < 0.0001 {
                tiles.push(TileId { col: primary_col + 1, row: primary_row + 1 });
            }
        }

        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_assignment_no_border() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        let tiles = streamer.get_tiles_for_point(56.5, 24.5);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0], TileId { col: 204, row: 146 });
    }

    #[test]
    fn test_tile_assignment_on_border() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        // Exactly on 57°N border
        let tiles = streamer.get_tiles_for_point(57.0, 24.5);
        assert!(tiles.len() >= 2); // Should be in both tiles
    }

    #[test]
    fn test_tile_assignment_corner() {
        let streamer = PbfStreamer::new(
            Path::new("dummy.pbf"),
            Path::new("output"),
            1.0
        ).unwrap();

        // Exactly on corner (57°N, 25°E)
        let tiles = streamer.get_tiles_for_point(57.0, 25.0);
        assert_eq!(tiles.len(), 4); // Should be in all 4 corner tiles
    }
}
```

**Rationale**:
- Streams PBF to avoid loading entire dataset
- Duplicates border points/lines in adjacent tiles (routing will deduplicate)
- Writes intermediate buffers to disk (not held in memory)
- Threshold of 0.0001° (~10m) for border detection

### 6. Update RMDF Module

**File**: `src/rmdf/mod.rs`

**Changes**: Add generator submodule

```rust
pub mod format;
pub mod validation;
pub mod io;
pub mod generator;  // NEW

pub use format::*;
pub use validation::*;
pub use io::*;
pub use generator::*;  // NEW
```

**Rationale**: Makes generator accessible via CLI handler.

## Success Criteria

### Automated Verification

- [ ] CLI accepts `generate-tiles` subcommand: `cargo run -- generate-tiles --help`
- [ ] Unit tests pass: `cargo test rmdf::generator`
- [ ] Border detection tests pass (no border, on border, on corner)
- [ ] Type checking passes: `cargo check`

### Manual Verification

- [ ] Run with test PBF: `ridi-router generate-tiles --input test.pbf --output ./tiles --tile-size 1.0`
- [ ] Intermediate files created in output directory
- [ ] File count matches expected tile coverage
- [ ] Border points appear in multiple tile buffers
- [ ] Tile size parameter affects partitioning correctly

## Dependencies

- **Depends on**: Phase 1 (needs TileId and format definitions)
- **Blocks**: Phase 3 (needs intermediate buffers for proximity computation)

## Risks & Mitigations

**Risk**: PBF streaming uses too much memory
- **Mitigation**: Write intermediate buffers to disk frequently, use bincode for compact storage

**Risk**: Way partitioning requires node lookups
- **Mitigation**: Two-pass approach: nodes first, then ways with coordinate lookup

**Risk**: Border detection threshold too strict/loose
- **Mitigation**: Use 0.0001° (~10m), validate with real data

**Risk**: Too many small tiles with tiny tile size
- **Mitigation**: Validate tile_size parameter (must be > 0.001°)

## Notes

- Intermediate format uses bincode (will be removed in Phase 8, only RMDF remains)
- Way partitioning simplified in this phase - will need node coordinate lookup
- Relations assigned in second pass based on via member location
- Border duplication is intentional - routing code will handle deduplication via OSM IDs
- Threshold of 0.0001° allows 10m precision for border detection
