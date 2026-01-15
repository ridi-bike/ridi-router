mod pbf_streamer;
mod intermediate;
mod proximity;

pub use pbf_streamer::PbfStreamer;
pub use proximity::ProximityComputer;

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

        // Phase 3: Proximity computation
        let proximity_computer = ProximityComputer::new(&self.input_file, self.tile_size_degrees)?;
        proximity_computer.compute_all(&tile_buffers, &self.output_dir)?;

        // Phase 4: RMDF writing (not implemented yet)

        Ok(())
    }
}
