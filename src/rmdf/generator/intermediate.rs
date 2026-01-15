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

    /// Save to disk as JSON (for debugging and compatibility)
    pub fn save_to_disk(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        let filename = format!("intermediate_{}_{}.json", self.tile_id.col, self.tile_id.row);
        let path = output_dir.join(filename);

        let file = std::fs::File::create(path)?;
        serde_json::to_writer(file, self)?;

        Ok(())
    }

    /// Load from disk
    pub fn load_from_disk(output_dir: &std::path::Path, tile_id: TileId) -> anyhow::Result<Self> {
        let filename = format!("intermediate_{}_{}.json", tile_id.col, tile_id.row);
        let path = output_dir.join(filename);

        let file = std::fs::File::open(path)?;
        let tile = serde_json::from_reader(file)?;

        Ok(tile)
    }

    /// Load from disk if exists, otherwise create new
    pub fn load_from_disk_if_exists(output_dir: &std::path::Path, tile_id: TileId) -> anyhow::Result<Self> {
        let filename = format!("intermediate_{}_{}.json", tile_id.col, tile_id.row);
        let path = output_dir.join(filename);

        if path.exists() {
            let file = std::fs::File::open(path)?;
            let tile = serde_json::from_reader(file)?;
            Ok(tile)
        } else {
            Ok(Self::new(tile_id))
        }
    }

    /// Merge another tile into this one
    pub fn merge(&mut self, other: IntermediateTile) -> anyhow::Result<()> {
        // Merge nodes (HashMap insert will replace if exists, but they should be identical)
        for (id, node) in other.nodes {
            self.nodes.insert(id, node);
        }

        // Merge ways (append, duplicates may occur but will be deduplicated later)
        self.ways.extend(other.ways);

        // Merge relations (append)
        self.relations.extend(other.relations);

        Ok(())
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

    /// Flush tiles to disk and clear memory (incremental saving)
    pub fn flush_to_disk(&mut self, output_dir: &std::path::Path) -> anyhow::Result<()> {
        for (tile_id, tile) in self.tiles.drain() {
            // Load existing tile from disk if it exists, otherwise create new
            let mut existing_tile = IntermediateTile::load_from_disk_if_exists(output_dir, tile_id)?;

            // Merge new data with existing
            existing_tile.merge(tile)?;

            // Write back to disk
            existing_tile.save_to_disk(output_dir)?;
        }

        Ok(())
    }
}

impl Default for TileBuffers {
    fn default() -> Self {
        Self::new()
    }
}
