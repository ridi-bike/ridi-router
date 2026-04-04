use std::{
    cmp::Eq,
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    marker::PhantomData,
};

use serde::{Deserialize, Serialize};

use crate::{
    map_data::line::LineDirection,
    rmdf::format::{TagSetRecord, TileId},
    router::rules::RouterRules,
};

use super::{line::MapDataLine, point::MapDataPoint};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagValueRef {
    pub tile_id: TileId,
    pub tag_value_idx: u32,
}
impl ElementTagValueRef {
    pub fn none(tile_id: TileId) -> Self {
        Self {
            tile_id,
            tag_value_idx: TagSetRecord::NONE,
        }
    }

    pub fn some(tile_id: TileId, tag_idx: u32) -> Self {
        Self {
            tile_id,
            tag_value_idx: tag_idx,
        }
    }

    pub fn from_idx(tile_id: TileId, tag_idx: u32) -> Self {
        if tag_idx == TagSetRecord::NONE {
            Self::none(tile_id)
        } else {
            Self::some(tile_id, tag_idx)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagSetRef {
    pub tile_id: TileId,
    pub tag_set_idx: u32,
}

impl ElementTagSetRef {
    pub fn new(tile_id: TileId, idx: u32) -> Self {
        Self {
            tile_id,
            tag_set_idx: idx,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagSet {
    pub name: ElementTagValueRef,
    pub hw_ref: ElementTagValueRef,
    pub highway: ElementTagValueRef,
    pub surface: ElementTagValueRef,
    pub smoothness: ElementTagValueRef,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTags {
    pub tag_values: Vec<smartstring::alias::String>,
    pub tag_sets: Vec<ElementTagSet>,
    tag_map: HashMap<smartstring::alias::String, u32>,
    tag_set_map: HashMap<ElementTagSet, u32>,
}

impl ElementTags {
    pub fn new() -> Self {
        Self::default()
    }
    #[allow(dead_code)]
    pub fn len(&self) -> (usize, usize) {
        (self.tag_values.len(), self.tag_sets.len())
    }
    #[allow(dead_code)]
    pub fn clear_maps(&mut self) {
        self.tag_set_map = HashMap::new();
        self.tag_map = HashMap::new();
    }
    pub fn get_or_create(
        &mut self,
        name: Option<&String>,
        hw_ref: Option<&String>,
        highway: Option<&String>,
        surface: Option<&String>,
        smoothness: Option<&String>,
    ) -> ElementTagSetRef {
        // LEGACY: This old tag deduplication system is not used in tile-based routing
        // Using placeholder TileId for compatibility with old code
        // This will be removed in Phase 8 cleanup
        let placeholder_tile = TileId { col: 0, row: 0 };

        let name_ref = self.get_tag_value_ref(name, placeholder_tile);
        let hw_ref_ref = self.get_tag_value_ref(hw_ref, placeholder_tile);
        let highway_ref = self.get_tag_value_ref(highway, placeholder_tile);
        let surface_ref = self.get_tag_value_ref(surface, placeholder_tile);
        let smoothness_ref = self.get_tag_value_ref(smoothness, placeholder_tile);

        let tag_set = ElementTagSet {
            name: name_ref,
            hw_ref: hw_ref_ref,
            highway: highway_ref,
            surface: surface_ref,
            smoothness: smoothness_ref,
        };
        let idx = match self.tag_set_map.get(&tag_set) {
            Some(i) => *i,
            None => {
                let new_idx = self.tag_sets.len() as u32;
                self.tag_set_map.insert(tag_set.clone(), new_idx);
                self.tag_sets.push(tag_set);
                new_idx
            }
        };
        ElementTagSetRef::new(placeholder_tile, idx)
    }
    fn get_tag_value_ref(&mut self, value: Option<&String>, tile_id: TileId) -> ElementTagValueRef {
        match value {
            None => ElementTagValueRef::none(tile_id),
            Some(v) => {
                let v = if v.ends_with("_link") {
                    v.replace("_link", "")
                } else {
                    v.to_string()
                };
                let idx = match self.tag_map.get(&smartstring::alias::String::from(&v)) {
                    Some(i) => *i,
                    None => {
                        let new_idx = self.tag_values.len() as u32;
                        self.tag_values.push(smartstring::alias::String::from(&v));
                        self.tag_map
                            .insert(smartstring::alias::String::from(&v), new_idx);
                        new_idx
                    }
                };
                ElementTagValueRef::some(tile_id, idx)
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MapDataElementRef<T> {
    tile_id: crate::rmdf::TileId,
    element_id: u64, // osm_id for points, line_index as u64 for lines
    _marker: PhantomData<T>,
}

impl<T> Display for MapDataElementRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ref(tile:{:?}, id:{})", self.tile_id, self.element_id)
    }
}

impl<T> MapDataElementRef<T> {
    pub fn new(tile_id: crate::rmdf::TileId, element_id: u64) -> Self {
        Self {
            tile_id,
            element_id,
            _marker: PhantomData,
        }
    }

    pub fn get_tile_id(&self) -> crate::rmdf::TileId {
        self.tile_id
    }

    pub fn get_element_id(&self) -> u64 {
        self.element_id
    }
}

impl<T> Clone for MapDataElementRef<T> {
    fn clone(&self) -> Self {
        Self {
            tile_id: self.tile_id,
            element_id: self.element_id,
            _marker: self._marker,
        }
    }
}

impl<T> PartialEq for MapDataElementRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.tile_id == other.tile_id && self.element_id == other.element_id
    }
}

impl<T> Eq for MapDataElementRef<T> {}

impl<T> Hash for MapDataElementRef<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tile_id.hash(state);
        self.element_id.hash(state);
    }
}

impl<T> Debug for MapDataElementRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ref(tile:{:?}, id:{})", self.tile_id, self.element_id)
    }
}

pub type MapDataLineRef = MapDataElementRef<MapDataLine>;
pub type MapDataPointRef = MapDataElementRef<MapDataPoint>;

pub struct MapDataGraph {
    tile_manager: std::sync::RwLock<crate::rmdf::TileManager>,
    // TODO: Implement proper tag loading from tiles
    // For now, keeping tags in memory for compatibility
    #[allow(dead_code)]
    tags: std::sync::RwLock<ElementTags>,
    // Test-only: in-memory storage for unit tests
    #[cfg(test)]
    test_points: std::sync::RwLock<HashMap<u64, MapDataPoint>>,
    #[cfg(test)]
    test_lines: std::sync::RwLock<HashMap<u64, MapDataLine>>,
}

impl MapDataGraph {
    pub fn new(tile_manager: crate::rmdf::TileManager) -> Self {
        Self {
            tile_manager: std::sync::RwLock::new(tile_manager),
            tags: std::sync::RwLock::new(ElementTags::new()),
            #[cfg(test)]
            test_points: std::sync::RwLock::new(HashMap::new()),
            #[cfg(test)]
            test_lines: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn open(
        tiles_dir: std::path::PathBuf,
        manifest: crate::rmdf::generator::manifest::TileManifest,
    ) -> Self {
        Self::new(crate::rmdf::TileManager::from_manifest(manifest, tiles_dir))
    }

    /// Test-only: Create a MapDataGraph with dummy TileManager for unit tests
    #[cfg(test)]
    pub fn new_test() -> Self {
        // Create a minimal TileManager with an empty manifest
        use crate::rmdf::generator::manifest::TileManifest;
        let manifest = TileManifest {
            version: "test".to_string(),
            tile_size_degrees: 1.0,
            format_version: 1,
            generated_at: "test".to_string(),
            source_files: Vec::new(),
            tiles: Vec::new(),
        };
        let tile_manager =
            crate::rmdf::TileManager::from_manifest(manifest, std::path::PathBuf::from("test"));
        Self {
            tile_manager: std::sync::RwLock::new(tile_manager),
            tags: std::sync::RwLock::new(ElementTags::new()),
            test_points: std::sync::RwLock::new(HashMap::new()),
            test_lines: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Test-only: Insert a point for testing
    #[cfg(test)]
    pub fn test_insert_point(&self, point: MapDataPoint) {
        self.test_points.write().unwrap().insert(point.id, point);
    }

    /// Test-only: Insert a line for testing
    #[cfg(test)]
    pub fn test_insert_line(&self, line: MapDataLine, line_id: u64) {
        self.test_lines.write().unwrap().insert(line_id, line);
    }

    /// Test-only: Add line reference to a point
    #[cfg(test)]
    pub fn test_add_line_to_point(&self, point_id: u64, line_ref: MapDataLineRef) {
        let mut points = self.test_points.write().unwrap();
        if let Some(point) = points.get_mut(&point_id) {
            point.lines.push(line_ref);
        }
    }

    /// Test-only: Get a MapDataPointRef by OSM ID for testing
    #[cfg(test)]
    pub fn test_get_point_ref_by_id(&self, id: &u64) -> Option<MapDataPointRef> {
        let points = self.test_points.read().unwrap();
        if points.contains_key(id) {
            // Use a dummy tile ID (0, 0) for test points
            Some(MapDataPointRef::new(
                crate::rmdf::TileId { col: 0, row: 0 },
                *id,
            ))
        } else {
            None
        }
    }

    /// Test-only: Get point from test storage
    #[cfg(test)]
    fn get_test_point(&self, osm_id: u64) -> Option<MapDataPoint> {
        self.test_points.read().unwrap().get(&osm_id).cloned()
    }

    /// Test-only: Get line from test storage
    #[cfg(test)]
    fn get_test_line(&self, line_id: u64) -> Option<MapDataLine> {
        self.test_lines.read().unwrap().get(&line_id).cloned()
    }

    /// Test-only: Add rule to a point
    #[cfg(test)]
    pub fn test_add_rule_to_point(&self, point_id: u64, rule: crate::map_data::rule::MapDataRule) {
        let mut points = self.test_points.write().unwrap();
        if let Some(point) = points.get_mut(&point_id) {
            point.rules.push(rule);
        }
    }

    // Get point data from tiles (or test storage in test mode)
    pub fn get_point_from_tiles(&self, tile_id: crate::rmdf::TileId, osm_id: u64) -> MapDataPoint {
        // In test mode, check test storage first
        #[cfg(test)]
        if let Some(point) = self.get_test_point(osm_id) {
            return point;
        }

        let mut tm = self.tile_manager.write().unwrap();

        // Get point data
        let point_record = tm
            .get_point_by_id(tile_id, osm_id)
            .expect("Failed to get point from tile");

        // Get adjacent lines for this point
        let adjacent = tm
            .get_adjacent_by_id(tile_id, osm_id)
            .expect("Failed to get adjacent lines");

        let lines: Vec<MapDataLineRef> = adjacent
            .iter()
            .map(|(line_tile_id, line_index, _, _)| {
                MapDataLineRef::new(*line_tile_id, *line_index as u64)
            })
            .collect();

        // Convert to MapDataPoint
        MapDataPoint {
            id: point_record.osm_id,
            lat: point_record.lat,
            lon: point_record.lon,
            lines,
            rules: Vec::new(), // TODO: Fetch rules from tiles
            residential_in_proximity: point_record.residential_in_proximity(),
            nogo_area: point_record.nogo_area(),
        }
    }

    // Get line data from tiles (or test storage in test mode)
    pub fn get_line_from_tiles(
        &self,
        tile_id: crate::rmdf::TileId,
        line_index: usize,
    ) -> MapDataLine {
        // In test mode, check test storage first
        #[cfg(test)]
        if let Some(line) = self.get_test_line(line_index as u64) {
            return line;
        }

        let mut tm = self.tile_manager.write().unwrap();
        let line_record = tm
            .get_line_by_index(tile_id, line_index)
            .expect("Failed to get line from tile");
        MapDataLine {
            points: (
                MapDataPointRef::new(tile_id, line_record.point_a_osm_id),
                MapDataPointRef::new(
                    tm.tile_id_for_coords(line_record.point_b_lat, line_record.point_b_lon),
                    line_record.point_b_osm_id,
                ),
            ),
            direction: match line_record.direction {
                0 => LineDirection::BothWays,
                1 => LineDirection::OneWay,
                2 => LineDirection::Roundabout,
                _ => LineDirection::BothWays,
            },
            tags: ElementTagSetRef::new(tile_id, line_record.tag_set_index),
        }
    }

    pub fn get_tag_value(&self, tag_value_ref: &ElementTagValueRef) -> Option<String> {
        if tag_value_ref.tag_value_idx == TagSetRecord::NONE {
            return None;
        }

        self.tile_manager
            .write()
            .unwrap()
            .get_tag_value(tag_value_ref.tile_id, tag_value_ref.tag_value_idx)
            .ok()
    }

    pub fn get_tag_set(&self, tag_set_ref: &ElementTagSetRef) -> ElementTagSet {
        let tag_set_record = self
            .tile_manager
            .write()
            .unwrap()
            .get_tag_set_record(tag_set_ref.tile_id, tag_set_ref.tag_set_idx)
            .unwrap_or_else(|_| TagSetRecord {
                name_idx: TagSetRecord::NONE,
                hw_ref_idx: TagSetRecord::NONE,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            });

        ElementTagSet {
            name: ElementTagValueRef::from_idx(tag_set_ref.tile_id, tag_set_record.name_idx),
            hw_ref: ElementTagValueRef::from_idx(tag_set_ref.tile_id, tag_set_record.hw_ref_idx),
            highway: ElementTagValueRef::from_idx(tag_set_ref.tile_id, tag_set_record.highway_idx),
            surface: ElementTagValueRef::from_idx(tag_set_ref.tile_id, tag_set_record.surface_idx),
            smoothness: ElementTagValueRef::from_idx(
                tag_set_ref.tile_id,
                tag_set_record.smoothness_idx,
            ),
        }
    }

    pub fn get_adjacent(
        &self,
        center_point: MapDataPointRef,
    ) -> Vec<(MapDataLineRef, MapDataPointRef)> {
        // In test mode, check test storage first
        #[cfg(test)]
        {
            if let Some(point) = self.get_test_point(center_point.get_element_id()) {
                let mut adjacent = Vec::new();
                for line_ref in &point.lines {
                    let line = self.get_line_from_tiles(
                        line_ref.get_tile_id(),
                        line_ref.get_element_id() as usize,
                    );
                    let other_point =
                        if line.points.0.get_element_id() == center_point.get_element_id() {
                            line.points.1.clone()
                        } else {
                            line.points.0.clone()
                        };
                    adjacent.push((line_ref.clone(), other_point));
                }
                return adjacent;
            }
        }

        let mut tm = self.tile_manager.write().unwrap();
        let adjacent = tm
            .get_adjacent_by_id(center_point.get_tile_id(), center_point.get_element_id())
            .expect("Failed to get adjacent points");
        adjacent
            .iter()
            .map(|(line_tile_id, line_index, other_tile_id, other_osm_id)| {
                (
                    MapDataLineRef::new(*line_tile_id, *line_index as u64),
                    MapDataPointRef::new(*other_tile_id, *other_osm_id),
                )
            })
            .collect()
    }

    pub fn get_closest_to_coords(
        &self,
        lat: f32,
        lon: f32,
        rules: &RouterRules,
        avoid_proximity_to_residential: bool,
        _limit_to_hw_tags: Option<&[&'static str]>,
    ) -> Option<MapDataPointRef> {
        let mut tm = self.tile_manager.write().unwrap();

        // Query TileManager for closest point
        let result = tm
            .get_closest_to_coords(
                lat,
                lon,
                rules,
                avoid_proximity_to_residential,
                _limit_to_hw_tags,
            )
            .ok()??;

        // Convert to MapDataPointRef
        Some(MapDataPointRef::new(result.0, result.1))
    }
}
