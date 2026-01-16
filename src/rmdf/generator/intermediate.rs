use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation};
use crate::rmdf::format::TileId;
use redb::{Database, TableDefinition, ReadableTable};
use bincode;

// redb table definitions for intermediate tile data
// Key: (col: u16, row: u16, osm_id: u64)
// Value: Serialized node/way/relation data
const TILE_NODES: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_nodes");
const TILE_WAYS: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_ways");
const TILE_RELATIONS: TableDefinition<(u16, u16, u64), &[u8]> = TableDefinition::new("tile_relations");

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

    /// Save tile data to redb database
    pub fn save_to_redb(&self, db: &Database) -> anyhow::Result<()> {
        let write_txn = db.begin_write()?;

        {
            let mut nodes_table = write_txn.open_table(TILE_NODES)?;
            for (osm_id, node) in &self.nodes {
                let key = (self.tile_id.col, self.tile_id.row, *osm_id);
                let value = bincode::serialize(node)?;
                nodes_table.insert(key, value.as_slice())?;
            }
        }

        {
            let mut ways_table = write_txn.open_table(TILE_WAYS)?;
            for way in &self.ways {
                let key = (self.tile_id.col, self.tile_id.row, way.id);
                let value = bincode::serialize(way)?;
                ways_table.insert(key, value.as_slice())?;
            }
        }

        {
            let mut relations_table = write_txn.open_table(TILE_RELATIONS)?;
            for relation in &self.relations {
                let key = (self.tile_id.col, self.tile_id.row, relation.id);
                let value = bincode::serialize(relation)?;
                relations_table.insert(key, value.as_slice())?;
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Load tile data from redb database with an existing read transaction
    pub fn load_from_redb_with_txn(read_txn: &redb::ReadTransaction, tile_id: TileId) -> anyhow::Result<Self> {
        let mut tile = IntermediateTile::new(tile_id);

        // Load nodes
        {
            let nodes_table = read_txn.open_table(TILE_NODES)?;
            let start_key = (tile_id.col, tile_id.row, 0u64);
            let end_key = (tile_id.col, tile_id.row, u64::MAX);

            for entry in nodes_table.range(start_key..=end_key)? {
                let (key_guard, value_guard) = entry?;
                let (_col, _row, osm_id) = key_guard.value();
                let node: OsmNode = bincode::deserialize(value_guard.value())?;
                tile.nodes.insert(osm_id, node);
            }
        }

        // Load ways
        {
            let ways_table = read_txn.open_table(TILE_WAYS)?;
            let start_key = (tile_id.col, tile_id.row, 0u64);
            let end_key = (tile_id.col, tile_id.row, u64::MAX);

            for entry in ways_table.range(start_key..=end_key)? {
                let (_key_guard, value_guard) = entry?;
                let way: OsmWay = bincode::deserialize(value_guard.value())?;
                tile.ways.push(way);
            }
        }

        // Load relations
        {
            let relations_table = read_txn.open_table(TILE_RELATIONS)?;
            let start_key = (tile_id.col, tile_id.row, 0u64);
            let end_key = (tile_id.col, tile_id.row, u64::MAX);

            for entry in relations_table.range(start_key..=end_key)? {
                let (_key_guard, value_guard) = entry?;
                let relation: OsmRelation = bincode::deserialize(value_guard.value())?;
                tile.relations.push(relation);
            }
        }

        Ok(tile)
    }

    /// Load tile data from redb database
    pub fn load_from_redb(db: &Database, tile_id: TileId) -> anyhow::Result<Self> {
        let read_txn = db.begin_read()?;
        Self::load_from_redb_with_txn(&read_txn, tile_id)
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

    /// Flush tiles to redb database and clear memory (incremental saving)
    pub fn flush_to_redb(&mut self, db: &Database) -> anyhow::Result<()> {
        for (_tile_id, tile) in self.tiles.drain() {
            tile.save_to_redb(db)?;
        }
        Ok(())
    }

    /// Discover all tile IDs from redb database
    pub fn discover_tiles_from_redb(db: &Database) -> anyhow::Result<Vec<TileId>> {
        let read_txn = db.begin_read()?;
        let nodes_table = read_txn.open_table(TILE_NODES)?;

        let mut tile_set = std::collections::HashSet::new();

        // Iterate all keys and extract unique (col, row) pairs
        for entry in nodes_table.iter()? {
            let (key_guard, _value_guard) = entry?;
            let (col, row, _osm_id) = key_guard.value();
            tile_set.insert(TileId { col, row });
        }

        let mut tile_ids: Vec<TileId> = tile_set.into_iter().collect();
        tile_ids.sort_by_key(|id| (id.col, id.row));

        Ok(tile_ids)
    }
}

impl Default for TileBuffers {
    fn default() -> Self {
        Self::new()
    }
}
