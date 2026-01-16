mod pbf_streamer;
mod intermediate;
mod proximity;
mod writer;
pub mod manifest;

pub use pbf_streamer::PbfStreamer;
pub use proximity::ProximityComputer;
pub use writer::RmdfWriter;
pub use manifest::{ManifestGenerator, TileManifest};

use std::path::PathBuf;
use anyhow::{Result, Context};
use tracing::info;
use redb::Database;

use crate::rmdf::format::TileId;
use super::generator::intermediate::{IntermediateTile, TileBuffers};

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
        // Phase 2: Stream, compute proximity, and partition
        let streamer = PbfStreamer::new(&self.input_file, &self.output_dir, self.tile_size_degrees)?;
        let (_tile_buffers, tiles_db) = streamer.partition()?;  // Returns empty (drained) buffers and database; proximity computed in Phase 2A

        // Discover tiles from database
        let tile_ids = TileBuffers::discover_tiles_from_redb(&tiles_db)?;
        info!("Discovered {} tiles from database", tile_ids.len());

        if tile_ids.is_empty() {
            anyhow::bail!("No tiles generated during partition phase");
        }

        // Phase 3: RMDF writing (proximity already computed in Phase 2)
        let writer = RmdfWriter::new(self.tile_size_degrees);

        info!("Writing RMDF files for {} tiles", tile_ids.len());

        // Create a single read transaction to reuse for all tile loads
        let read_txn = tiles_db.begin_read()
            .context("Failed to begin read transaction for tile loading")?;

        for (idx, tile_id) in tile_ids.iter().enumerate() {
            let intermediate = IntermediateTile::load_from_redb_with_txn(&read_txn, *tile_id)
                .with_context(|| format!("Failed to load tile {}/{} (ID: {:?})", idx + 1, tile_ids.len(), tile_id))?;
            let output_path = self.output_dir.join(tile_id.to_filename());
            writer.write_tile(&intermediate, &output_path)
                .with_context(|| format!("Failed to write tile {}/{} (ID: {:?})", idx + 1, tile_ids.len(), tile_id))?;
        }

        // Drop read transaction before closing database
        drop(read_txn);

        // Phase 4: Generate manifest
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        manifest_gen.generate(
            &self.output_dir,
            &tile_ids,
            self.input_file.to_str().unwrap_or("unknown"),
        )?;

        info!("Generated {} tiles with manifest", tile_ids.len());

        // Clean up intermediate tiles database
        drop(tiles_db);
        let tiles_db_path = self.output_dir.join("intermediate_tiles.redb");
        std::fs::remove_file(&tiles_db_path)
            .context("Failed to remove intermediate tiles database")?;
        info!("Cleaned up intermediate tiles database");

        Ok(())
    }
}
