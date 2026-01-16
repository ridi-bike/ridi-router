pub mod manifest;
mod pbf_streamer;
mod proximity;
mod writer;

pub use manifest::{ManifestGenerator, TileManifest};
pub use pbf_streamer::PbfStreamer;
pub use proximity::ProximityComputer;
pub use writer::RmdfWriter;

use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

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

        // Create PBF streamer
        let streamer =
            PbfStreamer::new(&self.input_file, &self.output_dir, self.tile_size_degrees)?;

        // Process all tiles in parallel
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
