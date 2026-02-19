pub mod manifest;
mod pbf_streamer;
mod writer;
pub mod intermediate;

pub use manifest::{ManifestGenerator, TileManifest};
pub use pbf_streamer::PbfStreamer;
pub use writer::RmdfWriter;

use crate::proximity::RasterizedProximityGrid;

use anyhow::{Context, Result};
use crate::osm_data::in_memory_pbf::InMemoryPbf;
use std::path::{Path, PathBuf};
use tracing::info;
use intermediate::GridStorage;

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
        info!("Starting RMDF tile generation");

        // Build in-memory PBF representation with pre-computed proximity flags
        // This uses SIMD-accelerated rasterized grid for O(1) flag lookups
        info!("Loading PBF file into memory with pre-computed proximity flags...");
        let pbf_data = InMemoryPbf::from_pbf_file_with_flags(&self.input_file)
            .context("Failed to load PBF file into memory with flags")?;

        // Create PBF streamer with reference to in-memory data
        let streamer = PbfStreamer::new(&pbf_data, &self.output_dir, self.tile_size_degrees);

        // Process all tiles in parallel (queries in-memory data)
        streamer.partition_parallel()?;

        // Generate manifest by discovering tiles from filesystem
        info!("Generating manifest");
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        let tile_ids = manifest_gen
            .discover_tiles(&self.output_dir)
            .context("Failed to discover tiles from filesystem")?;

        if tile_ids.is_empty() {
            tracing::warn!("No tiles were generated - all tiles may have been empty");
        } else {
            manifest_gen
                .generate(
                    &self.output_dir,
                    &tile_ids,
                    self.input_file.to_str().unwrap_or("unknown"),
                )
                .context("Failed to generate manifest")?;
        }

        info!("RMDF generation complete");
        Ok(())
    }
}


/// Discover all PBF files in a directory
fn discover_pbf_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut pbf_files = Vec::new();

    if !directory.exists() {
        anyhow::bail!("Directory does not exist: {:?}", directory);
    }

    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        // Check if it's a .pbf or .osm.pbf file
        if let Some(extension) = path.extension() {
            if extension == "pbf" {
                pbf_files.push(path);
            }
        }
    }

    // Sort for deterministic processing order
    pbf_files.sort();

    info!("Discovered {} PBF files in {:?}", pbf_files.len(), directory);
    Ok(pbf_files)
}

/// Generator for processing multiple PBF files with tile stitching
pub struct MultiPbfGenerator {
    input_directory: PathBuf,
    output_dir: PathBuf,
    tile_size_degrees: f32,
    db_path: PathBuf,
}

impl MultiPbfGenerator {
    /// Create a new multi-PBF generator
    pub fn new(
        input_directory: PathBuf,
        output_dir: PathBuf,
        tile_size_degrees: f32,
        db_path: PathBuf,
    ) -> Result<Self> {
        if tile_size_degrees <= 0.0 || tile_size_degrees > 180.0 {
            anyhow::bail!("Tile size must be between 0 and 180 degrees");
        }

        std::fs::create_dir_all(&output_dir)?;
        std::fs::create_dir_all(db_path.parent().unwrap_or(Path::new(".")))?;

        Ok(Self {
            input_directory,
            output_dir,
            tile_size_degrees,
            db_path,
        })
    }

    /// Generate tiles from all PBF files in the input directory
    ///
    /// This implements the multi-PBF processing pipeline:
    /// 1. Process each PBF independently (build grid, evaluate nodes)
    /// 2. Store grids and flagged nodes to redb
    /// 3. Identify overlap zones
    /// 4. Re-evaluate nodes in overlap zones with combined grids
    /// 5. Deduplicate and write final RMDF tiles
    pub fn generate(&self) -> Result<()> {
        info!("Starting multi-PBF tile generation from {:?}", self.input_directory);

        // Discover all PBF files
        let pbf_files = discover_pbf_files(&self.input_directory)?;
        if pbf_files.is_empty() {
            anyhow::bail!("No PBF files found in {:?}", self.input_directory);
        }

        // Open or create the redb database
        let db = redb::Database::create(&self.db_path)
            .context("Failed to create/open redb database")?;

        // Phase 1: Process each PBF file independently
        info!("Phase 1: Processing {} PBF files independently", pbf_files.len());
        for (pbf_id, pbf_path) in pbf_files.iter().enumerate() {
            self.process_single_pbf(&db, pbf_id as u64, pbf_path)?;
        }

        // Phase 2: Identify overlapping regions and affected zones
        info!("Phase 2: Identifying overlap zones");
        let overlap_zones = self.identify_overlap_zones(&db)?;
        info!("Found {} overlap zones", overlap_zones.len());

        // Phase 3: Re-evaluate nodes in overlap zones with combined grids
        if !overlap_zones.is_empty() {
            info!("Phase 3: Re-evaluating nodes in overlap zones");
            self.reevaluate_overlap_zones(&db, &overlap_zones)?;
        }

        // Phase 4: Deduplicate and write final tiles
        info!("Phase 4: Writing final RMDF tiles");
        self.write_final_tiles(&db, &pbf_files)?;

        // Generate manifest
        info!("Generating manifest");
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        let tile_ids = manifest_gen
            .discover_tiles(&self.output_dir)
            .context("Failed to discover tiles from filesystem")?;

        if tile_ids.is_empty() {
            tracing::warn!("No tiles were generated");
        } else {
            manifest_gen
                .generate(
                    &self.output_dir,
                    &tile_ids,
                    self.input_directory.to_str().unwrap_or("unknown"),
                )
                .context("Failed to generate manifest")?;
        }

        info!("Multi-PBF tile generation complete");
        Ok(())
    }

    /// Process a single PBF file and store results in redb
    fn process_single_pbf(&self, db: &redb::Database, pbf_id: u64, pbf_path: &Path) -> Result<()> {
        info!("Processing PBF {} ({:?})", pbf_id, pbf_path);

        // Load PBF with pre-computed flags
        let pbf_data = InMemoryPbf::from_pbf_file_with_flags(pbf_path)
            .with_context(|| format!("Failed to load PBF file: {:?}", pbf_path))?;

        // Get the proximity grid (need to expose it from InMemoryPbf)
        // For now, we'll need to modify InMemoryPbf to expose the grid
        // This is a placeholder - the actual implementation needs grid access

        // Store grid bounds
        let bounds = &pbf_data.bounds;
        let grid_bounds = intermediate::GridBounds {
            lon_min: bounds.lon_min.unwrap_or(0.0) as f32,
            lat_min: bounds.lat_min.unwrap_or(0.0) as f32,
            lon_max: bounds.lon_max.unwrap_or(0.0) as f32,
            lat_max: bounds.lat_max.unwrap_or(0.0) as f32,
        };

        // Create PBF streamer and partition tiles
        let streamer = PbfStreamer::new(&pbf_data, &self.output_dir, self.tile_size_degrees);
        streamer.partition_parallel()?;

        // Store metadata
        // Note: Grid storage would happen here once we have grid access from InMemoryPbf
        GridStorage::store_grid(
            db,
            pbf_id,
            pbf_path.to_str().unwrap_or(""),
            &[], // Placeholder for grid bytes
            &grid_bounds,
        )?;

        info!("Completed processing PBF {}", pbf_id);
        Ok(())
    }

    /// Identify zones where PBF grids overlap
    fn identify_overlap_zones(&self, db: &redb::Database) -> Result<Vec<intermediate::GridBounds>> {
        let all_bounds = GridStorage::load_all_bounds(db)?;
        let mut overlap_zones = Vec::new();

        // Find all pairwise overlaps
        for i in 0..all_bounds.len() {
            for j in (i + 1)..all_bounds.len() {
                let (_, bounds_a) = &all_bounds[i];
                let (_, bounds_b) = &all_bounds[j];

                if bounds_a.overlaps(bounds_b) {
                    // Calculate intersection bounds
                    let intersection = intermediate::GridBounds {
                        lon_min: bounds_a.lon_min.max(bounds_b.lon_min),
                        lat_min: bounds_a.lat_min.max(bounds_b.lat_min),
                        lon_max: bounds_a.lon_max.min(bounds_b.lon_max),
                        lat_max: bounds_a.lat_max.min(bounds_b.lat_max),
                    };

                    // Add 500m buffer for proximity check radius
                    let buffer_deg = 500.0 / 111_000.0; // ~500m in degrees
                    let buffered = intermediate::GridBounds {
                        lon_min: intersection.lon_min - buffer_deg as f32,
                        lat_min: intersection.lat_min - buffer_deg as f32,
                        lon_max: intersection.lon_max + buffer_deg as f32,
                        lat_max: intersection.lat_max + buffer_deg as f32,
                    };

                    overlap_zones.push(buffered);
                }
            }
        }

        Ok(overlap_zones)
    }

    /// Re-evaluate nodes in overlap zones with combined grids
    fn reevaluate_overlap_zones(
        &self,
        _db: &redb::Database,
        _overlap_zones: &[intermediate::GridBounds],
    ) -> Result<()> {
        // TODO: Implement grid combination and node re-evaluation
        // This requires:
        // 1. Load grids that cover each overlap zone
        // 2. Merge grids using MAX for sectors, OR for military
        // 3. Query nodes in overlap zones from redb
        // 4. Re-evaluate flags with combined grid
        // 5. Update nodes in redb
        info!("Overlap zone re-evaluation not yet fully implemented");
        Ok(())
    }

    /// Write final deduplicated tiles
    fn write_final_tiles(&self, _db: &redb::Database, _pbf_files: &[PathBuf]) -> Result<()> {
        // Tiles are already written by PbfStreamer in phase 1
        // In a full implementation, we would:
        // 1. Deduplicate nodes/ways/relations across tiles
        // 2. Write combined tiles
        info!("Final tile writing complete (tiles written during phase 1)");
        Ok(())
    }
}
