use std::collections::HashMap;
use anyhow::Context;
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

// Grid storage tables for multi-PBF support
// Key: PBF file ID (u64)
// Value: Serialized grid (header + cells)
const PROXIMITY_GRIDS: TableDefinition<u64, &[u8]> = TableDefinition::new("proximity_grids");

// PBF file metadata
// Key: PBF file ID (u64)
// Value: PBF file path as string
const PBF_FILES: TableDefinition<u64, &str> = TableDefinition::new("pbf_files");

// Grid bounds for overlap detection
// Key: PBF file ID (u64)
// Value: Serialized bounds (lon_min, lat_min, lon_max, lat_max as f32)
const PBF_BOUNDS: TableDefinition<u64, &[u8]> = TableDefinition::new("pbf_bounds");

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

    /// Deduplicate ways and relations by OSM ID
    ///
    /// After merging tiles from multiple PBFs, we may have duplicate
    /// ways/relations with the same OSM ID. This removes duplicates.
    pub fn deduplicate(&mut self) {
        use std::collections::HashSet;

        // Deduplicate ways by ID
        let mut seen_way_ids = HashSet::new();
        let mut unique_ways = Vec::new();
        for way in self.ways.drain(..) {
            if seen_way_ids.insert(way.id) {
                unique_ways.push(way);
            }
        }
        self.ways = unique_ways;

        // Deduplicate relations by ID
        let mut seen_relation_ids = HashSet::new();
        let mut unique_relations = Vec::new();
        for relation in self.relations.drain(..) {
            if seen_relation_ids.insert(relation.id) {
                unique_relations.push(relation);
            }
        }
        self.relations = unique_relations;
    }

    /// Check if this tile has any content
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.ways.is_empty() && self.relations.is_empty()
    }

    /// Get counts of elements in this tile
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.nodes.len(), self.ways.len(), self.relations.len())
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
            let nodes_table = read_txn.open_table(TILE_NODES)
                .context("Failed to open TILE_NODES table")?;
            let start_key = (tile_id.col, tile_id.row, 0u64);
            let end_key = (tile_id.col, tile_id.row, u64::MAX);

            for entry in nodes_table.range(start_key..=end_key)
                .context("Failed to create range iterator for nodes")? {
                let (key_guard, value_guard) = entry
                    .context("Failed to read node entry from database")?;
                let (_col, _row, osm_id) = key_guard.value();
                let node: OsmNode = bincode::deserialize(value_guard.value())
                    .context("Failed to deserialize node")?;
                tile.nodes.insert(osm_id, node);
            }
        }

        // Load ways
        {
            let ways_table = read_txn.open_table(TILE_WAYS)
                .context("Failed to open TILE_WAYS table")?;
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
            let relations_table = read_txn.open_table(TILE_RELATIONS)
                .context("Failed to open TILE_RELATIONS table")?;
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
        if self.tiles.is_empty() {
            return Ok(());
        }

        // Create a single write transaction for all tiles to avoid creating hundreds of transactions
        let write_txn = db.begin_write()?;

        {
            let mut nodes_table = write_txn.open_table(TILE_NODES)?;
            let mut ways_table = write_txn.open_table(TILE_WAYS)?;
            let mut relations_table = write_txn.open_table(TILE_RELATIONS)?;

            for (_tile_id, tile) in self.tiles.drain() {
                // Insert nodes
                for (osm_id, node) in &tile.nodes {
                    let key = (tile.tile_id.col, tile.tile_id.row, *osm_id);
                    let value = bincode::serialize(node)?;
                    nodes_table.insert(key, value.as_slice())?;
                }

                // Insert ways
                for way in &tile.ways {
                    let key = (tile.tile_id.col, tile.tile_id.row, way.id);
                    let value = bincode::serialize(way)?;
                    ways_table.insert(key, value.as_slice())?;
                }

                // Insert relations
                for relation in &tile.relations {
                    let key = (tile.tile_id.col, tile.tile_id.row, relation.id);
                    let value = bincode::serialize(relation)?;
                    relations_table.insert(key, value.as_slice())?;
                }
            }
        }

        write_txn.commit()?;
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


/// Grid bounds stored in PBF_BOUNDS table
#[derive(Debug, Clone, Copy)]
pub struct GridBounds {
    pub lon_min: f32,
    pub lat_min: f32,
    pub lon_max: f32,
    pub lat_max: f32,
}

impl GridBounds {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.lon_min.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.lat_min.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.lon_max.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.lat_max.to_le_bytes());
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }
        Some(Self {
            lon_min: f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            lat_min: f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            lon_max: f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            lat_max: f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        })
    }

    /// Check if this bounds overlaps with another
    pub fn overlaps(&self, other: &GridBounds) -> bool {
        !(self.lon_max < other.lon_min
            || self.lon_min > other.lon_max
            || self.lat_max < other.lat_min
            || self.lat_min > other.lat_max)
    }
}

/// Helper functions for grid storage
pub struct GridStorage;

impl GridStorage {
    /// Store a proximity grid in the database
    pub fn store_grid(
        db: &Database,
        pbf_id: u64,
        pbf_path: &str,
        grid_bytes: &[u8],
        bounds: &GridBounds,
    ) -> anyhow::Result<()> {
        let write_txn = db.begin_write()?;

        {
            let mut grids_table = write_txn.open_table(PROXIMITY_GRIDS)?;
            let mut files_table = write_txn.open_table(PBF_FILES)?;
            let mut bounds_table = write_txn.open_table(PBF_BOUNDS)?;

            grids_table.insert(pbf_id, grid_bytes)?;
            files_table.insert(pbf_id, pbf_path)?;
            bounds_table.insert(pbf_id, bounds.to_bytes().as_slice())?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Load a proximity grid from the database
    pub fn load_grid(db: &Database, pbf_id: u64) -> anyhow::Result<Option<Vec<u8>>> {
        let read_txn = db.begin_read()?;
        let grids_table = read_txn.open_table(PROXIMITY_GRIDS)?;

        if let Some(entry) = grids_table.get(pbf_id)? {
            Ok(Some(entry.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Load all grid bounds from the database
    pub fn load_all_bounds(db: &Database) -> anyhow::Result<Vec<(u64, GridBounds)>> {
        let read_txn = db.begin_read()?;
        let bounds_table = read_txn.open_table(PBF_BOUNDS)?;

        let mut result = Vec::new();
        for entry in bounds_table.iter()? {
            let (key_guard, value_guard) = entry?;
            let pbf_id = key_guard.value();
            if let Some(bounds) = GridBounds::from_bytes(value_guard.value()) {
                result.push((pbf_id, bounds));
            }
        }

        Ok(result)
    }

    /// Find PBF files whose bounds overlap with a given region
    pub fn find_overlapping_pbfs(
        db: &Database,
        region: &GridBounds,
    ) -> anyhow::Result<Vec<u64>> {
        let all_bounds = Self::load_all_bounds(db)?;
        let overlapping: Vec<u64> = all_bounds
            .into_iter()
            .filter(|(_, bounds)| region.overlaps(bounds))
            .map(|(pbf_id, _)| pbf_id)
            .collect();
        Ok(overlapping)
    }

    /// Load multiple grids and return them with their bounds
    pub fn load_grids_for_region(
        db: &Database,
        region: &GridBounds,
    ) -> anyhow::Result<Vec<(u64, GridBounds, Vec<u8>)>> {
        let overlapping = Self::find_overlapping_pbfs(db, region)?;
        let mut result = Vec::new();

        for pbf_id in overlapping {
            if let Some(grid_bytes) = Self::load_grid(db, pbf_id)? {
                let all_bounds = Self::load_all_bounds(db)?;
                if let Some((_, bounds)) = all_bounds.into_iter().find(|(id, _)| *id == pbf_id) {
                    result.push((pbf_id, bounds, grid_bytes));
                }
            }
        }

        Ok(result)
    }
}

/// Find overlapping cells between grids
///
/// Returns a map from global cell coordinates (col, row) to list of (grid_id, local_cell_idx)
pub fn find_overlapping_cells(
    grids: &[(u64, GridBounds, crate::proximity::RasterizedProximityGrid)],
) -> std::collections::HashMap<(i32, i32), Vec<(u64, usize)>> {
    use crate::proximity::GRID_CELL_SIZE_DEG;

    let mut overlaps: std::collections::HashMap<(i32, i32), Vec<(u64, usize)>> = std::collections::HashMap::new();

    for (grid_id, _bounds, grid) in grids {
        for local_idx in 0..grid.cell_count() {
            let (lat, lon) = grid.cell_center(local_idx);

            // Convert to global cell coordinates
            let global_col = ((lon + 180.0) / GRID_CELL_SIZE_DEG).floor() as i32;
            let global_row = ((lat + 90.0) / GRID_CELL_SIZE_DEG).floor() as i32;

            overlaps
                .entry((global_col, global_row))
                .or_default()
                .push((*grid_id, local_idx));
        }
    }

    // Only return cells that appear in multiple grids
    overlaps.into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .collect()
}

/// Build a combined grid from multiple overlapping grids
///
/// Uses MAX for sectors (avoid double-counting) and OR for military
pub fn build_combined_grid(
    grids: &[(u64, GridBounds, crate::proximity::RasterizedProximityGrid)],
    region: &GridBounds,
) -> crate::proximity::RasterizedProximityGrid {
    use crate::proximity::RasterizedProximityGrid;
    use crate::osm_data::in_memory_pbf::PbfBounds;

    // Create a new grid for the region
    let bounds = PbfBounds {
        lat_min: Some(region.lat_min as f64),
        lat_max: Some(region.lat_max as f64),
        lon_min: Some(region.lon_min as f64),
        lon_max: Some(region.lon_max as f64),
    };

    let mut combined = RasterizedProximityGrid::new(&bounds);

    // Find overlapping cells and merge
    let overlaps = find_overlapping_cells(grids);

    for (_global_coord, sources) in overlaps {
        // Get the cell in combined grid (need to map global to local)
        // For simplicity, we merge each source cell into combined
        for (grid_id, local_idx) in sources {
            // Find the source grid
            if let Some((_, _, source_grid)) = grids.iter().find(|(id, _, _)| *id == grid_id) {
                if let Some(source_cell) = source_grid.cells().get(local_idx) {
                    let (lat, lon) = source_grid.cell_center(local_idx);
                    if let Some(combined_cell) = combined.get_cell_mut(lat, lon) {
                        combined_cell.merge(source_cell);
                    }
                }
            }
        }
    }

    // Also copy non-overlapping cells from each grid
    for (_, _, source_grid) in grids {
        for local_idx in 0..source_grid.cell_count() {
            let (lat, lon) = source_grid.cell_center(local_idx);

            // Check if this cell is in the combined grid's bounds
            if let Some(combined_cell) = combined.get_cell_mut(lat, lon) {
                if let Some(source_cell) = source_grid.cells().get(local_idx) {
                    // Only merge if combined cell is still empty (not already merged)
                    if combined_cell.total_residential_area() == 0.0 {
                        combined_cell.merge(source_cell);
                    }
                }
            }
        }
    }

    combined
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_data::osm::{OsmNode, OsmWay, OsmRelation};
    use crate::rmdf::format::TileId;

    fn make_test_tile() -> IntermediateTile {
        IntermediateTile::new(TileId { col: 0, row: 0 })
    }

    fn make_test_node(id: u64) -> OsmNode {
        OsmNode {
            id,
            lat: 50.0,
            lon: 10.0,
            residential_in_proximity: false,
            nogo_area: false,
        }
    }

    fn make_test_way(id: u64) -> OsmWay {
        OsmWay {
            id,
            point_ids: vec![1, 2, 3],
            tags: None,
        }
    }

    fn make_test_relation(id: u64) -> OsmRelation {
        OsmRelation {
            id,
            members: vec![],
            tags: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_intermediate_tile_new() {
        let tile = IntermediateTile::new(TileId { col: 5, row: 10 });
        assert_eq!(tile.tile_id.col, 5);
        assert_eq!(tile.tile_id.row, 10);
        assert!(tile.nodes.is_empty());
        assert!(tile.ways.is_empty());
        assert!(tile.relations.is_empty());
    }

    #[test]
    fn test_intermediate_tile_is_empty() {
        let mut tile = make_test_tile();
        assert!(tile.is_empty());

        tile.add_node(make_test_node(1));
        assert!(!tile.is_empty());

        let mut tile2 = make_test_tile();
        tile2.add_way(make_test_way(1));
        assert!(!tile2.is_empty());

        let mut tile3 = make_test_tile();
        tile3.add_relation(make_test_relation(1));
        assert!(!tile3.is_empty());
    }

    #[test]
    fn test_intermediate_tile_counts() {
        let mut tile = make_test_tile();
        
        let (nodes, ways, relations) = tile.counts();
        assert_eq!((nodes, ways, relations), (0, 0, 0));

        tile.add_node(make_test_node(1));
        tile.add_node(make_test_node(2));
        tile.add_way(make_test_way(10));
        tile.add_relation(make_test_relation(100));

        let (nodes, ways, relations) = tile.counts();
        assert_eq!((nodes, ways, relations), (2, 1, 1));
    }

    #[test]
    fn test_intermediate_tile_deduplicate_ways() {
        let mut tile = make_test_tile();
        
        // Add duplicate ways (same ID)
        tile.add_way(make_test_way(1));
        tile.add_way(make_test_way(1));
        tile.add_way(make_test_way(1));
        tile.add_way(make_test_way(2));
        tile.add_way(make_test_way(2));
        tile.add_way(make_test_way(3));

        assert_eq!(tile.ways.len(), 6);
        
        tile.deduplicate();
        
        assert_eq!(tile.ways.len(), 3);
        let way_ids: std::collections::HashSet<u64> = tile.ways.iter().map(|w| w.id).collect();
        assert!(way_ids.contains(&1));
        assert!(way_ids.contains(&2));
        assert!(way_ids.contains(&3));
    }

    #[test]
    fn test_intermediate_tile_deduplicate_relations() {
        let mut tile = make_test_tile();
        
        // Add duplicate relations
        tile.add_relation(make_test_relation(1));
        tile.add_relation(make_test_relation(1));
        tile.add_relation(make_test_relation(2));

        assert_eq!(tile.relations.len(), 3);
        
        tile.deduplicate();
        
        assert_eq!(tile.relations.len(), 2);
    }

    #[test]
    fn test_intermediate_tile_deduplicate_no_duplicates() {
        let mut tile = make_test_tile();
        
        tile.add_way(make_test_way(1));
        tile.add_way(make_test_way(2));
        tile.add_way(make_test_way(3));
        tile.add_relation(make_test_relation(10));
        tile.add_relation(make_test_relation(20));

        tile.deduplicate();

        assert_eq!(tile.ways.len(), 3);
        assert_eq!(tile.relations.len(), 2);
    }

    #[test]
    fn test_intermediate_tile_deduplicate_empty() {
        let mut tile = make_test_tile();
        tile.deduplicate();
        assert!(tile.is_empty());
    }

    #[test]
    fn test_grid_bounds_to_from_bytes() {
        let bounds = GridBounds {
            lon_min: 10.5,
            lat_min: 50.5,
            lon_max: 11.5,
            lat_max: 51.5,
        };

        let bytes = bounds.to_bytes();
        assert_eq!(bytes.len(), 16);

        let restored = GridBounds::from_bytes(&bytes).expect("Should deserialize");
        assert!((restored.lon_min - bounds.lon_min).abs() < f32::EPSILON);
        assert!((restored.lat_min - bounds.lat_min).abs() < f32::EPSILON);
        assert!((restored.lon_max - bounds.lon_max).abs() < f32::EPSILON);
        assert!((restored.lat_max - bounds.lat_max).abs() < f32::EPSILON);
    }

    #[test]
    fn test_grid_bounds_from_bytes_too_small() {
        assert!(GridBounds::from_bytes(&[]).is_none());
        assert!(GridBounds::from_bytes(&[0, 1, 2]).is_none());
        assert!(GridBounds::from_bytes(&[0; 15]).is_none());
        assert!(GridBounds::from_bytes(&[0; 16]).is_some());
    }

    #[test]
    fn test_grid_bounds_overlaps_true() {
        let a = GridBounds {
            lon_min: 0.0, lat_min: 0.0, lon_max: 10.0, lat_max: 10.0,
        };
        let b = GridBounds {
            lon_min: 5.0, lat_min: 5.0, lon_max: 15.0, lat_max: 15.0,
        };
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a)); // Symmetric

        // One contains the other
        let c = GridBounds {
            lon_min: 2.0, lat_min: 2.0, lon_max: 8.0, lat_max: 8.0,
        };
        assert!(a.overlaps(&c));
        assert!(c.overlaps(&a));

        // Edge touching (inclusive)
        let d = GridBounds {
            lon_min: 10.0, lat_min: 10.0, lon_max: 20.0, lat_max: 20.0,
        };
        assert!(a.overlaps(&d)); // Touching at corner
    }

    #[test]
    fn test_grid_bounds_overlaps_false() {
        let a = GridBounds {
            lon_min: 0.0, lat_min: 0.0, lon_max: 10.0, lat_max: 10.0,
        };
        
        // Completely separate
        let b = GridBounds {
            lon_min: 20.0, lat_min: 20.0, lon_max: 30.0, lat_max: 30.0,
        };
        assert!(!a.overlaps(&b));
        
        // Adjacent but not touching (b just past a's max)
        let c = GridBounds {
            lon_min: 10.1, lat_min: 0.0, lon_max: 20.0, lat_max: 10.0,
        };
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn test_find_overlapping_cells_no_overlap() {
        use crate::proximity::RasterizedProximityGrid;
        use crate::osm_data::in_memory_pbf::PbfBounds;

        // Create two grids that don't overlap
        let bounds1 = PbfBounds {
            lat_min: Some(50.0), lat_max: Some(50.01),
            lon_min: Some(10.0), lon_max: Some(10.01),
        };
        let bounds2 = PbfBounds {
            lat_min: Some(60.0), lat_max: Some(60.01),
            lon_min: Some(20.0), lon_max: Some(20.01),
        };

        let grid1 = RasterizedProximityGrid::new(&bounds1);
        let grid2 = RasterizedProximityGrid::new(&bounds2);

        let grid_bounds1 = GridBounds {
            lon_min: 10.0, lat_min: 50.0, lon_max: 10.01, lat_max: 50.01,
        };
        let grid_bounds2 = GridBounds {
            lon_min: 20.0, lat_min: 60.0, lon_max: 20.01, lat_max: 60.01,
        };

        let grids: Vec<(u64, GridBounds, RasterizedProximityGrid)> = vec![
            (1, grid_bounds1, grid1),
            (2, grid_bounds2, grid2),
        ];

        let overlaps = find_overlapping_cells(&grids);
        assert!(overlaps.is_empty(), "Non-overlapping grids should have no overlapping cells");
    }

    #[test]
    fn test_find_overlapping_cells_with_overlap() {
        use crate::proximity::RasterizedProximityGrid;
        use crate::osm_data::in_memory_pbf::PbfBounds;

        // Create two grids that overlap
        let bounds1 = PbfBounds {
            lat_min: Some(50.0), lat_max: Some(50.02),
            lon_min: Some(10.0), lon_max: Some(10.02),
        };
        let bounds2 = PbfBounds {
            lat_min: Some(50.01), lat_max: Some(50.03),
            lon_min: Some(10.01), lon_max: Some(10.03),
        };

        let grid1 = RasterizedProximityGrid::new(&bounds1);
        let grid2 = RasterizedProximityGrid::new(&bounds2);

        let grid_bounds1 = GridBounds {
            lon_min: 10.0, lat_min: 50.0, lon_max: 10.02, lat_max: 50.02,
        };
        let grid_bounds2 = GridBounds {
            lon_min: 10.01, lat_min: 50.01, lon_max: 10.03, lat_max: 50.03,
        };

        let grids: Vec<(u64, GridBounds, RasterizedProximityGrid)> = vec![
            (1, grid_bounds1, grid1),
            (2, grid_bounds2, grid2),
        ];

        let overlaps = find_overlapping_cells(&grids);
        
        // There should be some overlapping cells in the intersection
        assert!(!overlaps.is_empty(), "Overlapping grids should have overlapping cells");
        
        // Each overlap should have exactly 2 sources (one from each grid)
        for (_, sources) in &overlaps {
            assert_eq!(sources.len(), 2);
        }
    }

    #[test]
    fn test_build_combined_grid() {
        use crate::proximity::RasterizedProximityGrid;
        use crate::osm_data::in_memory_pbf::PbfBounds;

        // Create two overlapping grids
        let bounds1 = PbfBounds {
            lat_min: Some(50.0), lat_max: Some(50.02),
            lon_min: Some(10.0), lon_max: Some(10.02),
        };
        let bounds2 = PbfBounds {
            lat_min: Some(50.01), lat_max: Some(50.03),
            lon_min: Some(10.01), lon_max: Some(10.03),
        };

        let rasterizer1 = crate::proximity::area_rasterizer::AreaRasterizer::new(&bounds1);
        let rasterizer2 = crate::proximity::area_rasterizer::AreaRasterizer::new(&bounds2);
        
        let grid1 = rasterizer1.into_grid();
        let grid2 = rasterizer2.into_grid();

        let grid_bounds1 = GridBounds {
            lon_min: 10.0, lat_min: 50.0, lon_max: 10.02, lat_max: 50.02,
        };
        let grid_bounds2 = GridBounds {
            lon_min: 10.01, lat_min: 50.01, lon_max: 10.03, lat_max: 50.03,
        };

        // Region covering the overlap
        let region = GridBounds {
            lon_min: 10.01, lat_min: 50.01, lon_max: 10.02, lat_max: 50.02,
        };

        let grids: Vec<(u64, GridBounds, RasterizedProximityGrid)> = vec![
            (1, grid_bounds1, grid1),
            (2, grid_bounds2, grid2),
        ];

        let combined = build_combined_grid(&grids, &region);
        
        // Combined grid should cover the region
        let (lon_min, lat_min, lon_max, lat_max) = combined.bounds();
        assert!(lon_min <= region.lon_min);
        assert!(lat_min <= region.lat_min);
        assert!(lon_max >= region.lon_max);
        assert!(lat_max >= region.lat_max);
    }
}
