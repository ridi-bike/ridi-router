mod pbf_streamer;
mod intermediate;
mod proximity;
mod writer;
mod manifest;

pub use pbf_streamer::PbfStreamer;
pub use proximity::ProximityComputer;
pub use writer::RmdfWriter;
pub use manifest::ManifestGenerator;

use std::path::PathBuf;
use anyhow::Result;
use tracing::info;

use crate::rmdf::format::TileId;
use super::generator::intermediate::IntermediateTile;

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

        // Phase 3: Proximity computation
        let proximity_computer = ProximityComputer::new(&self.input_file, self.tile_size_degrees)?;
        proximity_computer.compute_all(&tile_buffers, &self.output_dir)?;

        // Phase 4: RMDF writing
        let writer = RmdfWriter::new(self.tile_size_degrees);
        let tile_ids: Vec<TileId> = tile_buffers.tiles.keys().cloned().collect();

        info!("Writing RMDF files for {} tiles", tile_ids.len());
        for tile_id in &tile_ids {
            let intermediate = IntermediateTile::load_from_disk(&self.output_dir, *tile_id)?;
            let output_path = self.output_dir.join(tile_id.to_filename());
            writer.write_tile(&intermediate, &output_path)?;
        }

        // Generate manifest
        let manifest_gen = ManifestGenerator::new(self.tile_size_degrees);
        manifest_gen.generate(
            &self.output_dir,
            &tile_ids,
            self.input_file.to_str().unwrap_or("unknown"),
        )?;

        info!("Generated {} tiles with manifest", tile_ids.len());

        Ok(())
    }
}
