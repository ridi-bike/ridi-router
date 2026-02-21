use std::{
    cmp::Eq,
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    marker::PhantomData,
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

#[cfg(feature = "debug-with-postgres")]
use crate::map_data::debug_writer::MapDebugWriter;
#[cfg(feature = "debug-with-postgres")]
use geo::{Coord, LineString};

use crate::{
    rmdf::format::{TagSetRecord, TileId},
    router::rules::RouterRules,
};

use super::{line::MapDataLine, point::MapDataPoint};

#[derive(PartialEq, Eq, Hash)]
enum AvoidTag {
    Highway(String),
    Surface(String),
    Smoothness(String),
}

pub static MAP_DATA_GRAPH: OnceLock<MapDataGraph> = OnceLock::new();

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

    pub fn get(&self) -> Option<String> {
        if self.tag_value_idx == TagSetRecord::NONE {
            return None;
        }

        MapDataGraph::get()
            .tile_manager
            .write()
            .unwrap()
            .get_tag_value(self.tile_id, self.tag_value_idx)
            .ok()
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

    pub fn get(&self) -> ElementTagSet {
        let tag_set_record = MapDataGraph::get()
            .tile_manager
            .write()
            .unwrap()
            .get_tag_set_record(self.tile_id, self.tag_set_idx)
            .unwrap_or_else(|_| TagSetRecord {
                name_idx: TagSetRecord::NONE,
                hw_ref_idx: TagSetRecord::NONE,
                highway_idx: TagSetRecord::NONE,
                surface_idx: TagSetRecord::NONE,
                smoothness_idx: TagSetRecord::NONE,
            });

        // Construct ElementTagSet with tile-aware refs
        ElementTagSet {
            name: ElementTagValueRef::from_idx(self.tile_id, tag_set_record.name_idx),
            hw_ref: ElementTagValueRef::from_idx(self.tile_id, tag_set_record.hw_ref_idx),
            highway: ElementTagValueRef::from_idx(self.tile_id, tag_set_record.highway_idx),
            surface: ElementTagValueRef::from_idx(self.tile_id, tag_set_record.surface_idx),
            smoothness: ElementTagValueRef::from_idx(self.tile_id, tag_set_record.smoothness_idx),
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

impl ElementTagSet {
    pub fn name(&self) -> Option<String> {
        self.name.get()
    }
    pub fn hw_ref(&self) -> Option<String> {
        self.hw_ref.get()
    }
    pub fn highway(&self) -> Option<String> {
        self.highway.get()
    }
    pub fn surface(&self) -> Option<String> {
        self.surface.get()
    }
    pub fn smoothness(&self) -> Option<String> {
        self.smoothness.get()
    }
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
    pub fn len(&self) -> (usize, usize) {
        (self.tag_values.len(), self.tag_sets.len())
    }
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

pub trait MapDataElement: Debug + Display {
    fn get_from_tiles(tile_id: crate::rmdf::TileId, element_id: u64) -> Self;
}
impl MapDataElement for MapDataPoint {
    fn get_from_tiles(tile_id: crate::rmdf::TileId, osm_id: u64) -> MapDataPoint {
        MapDataGraph::get().get_point_from_tiles(tile_id, osm_id)
    }
}
impl MapDataElement for MapDataLine {
    fn get_from_tiles(tile_id: crate::rmdf::TileId, line_index: u64) -> MapDataLine {
        MapDataGraph::get().get_line_from_tiles(tile_id, line_index as usize)
    }
}

#[derive(Serialize, Deserialize)]
pub struct MapDataElementRef<T: MapDataElement> {
    tile_id: crate::rmdf::TileId,
    element_id: u64, // osm_id for points, line_index as u64 for lines
    _marker: PhantomData<T>,
}

impl<T: MapDataElement + 'static> Display for MapDataElementRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ref(tile:{:?}, id:{})", self.tile_id, self.element_id)
    }
}

impl<T: MapDataElement> MapDataElementRef<T> {
    pub fn new(tile_id: crate::rmdf::TileId, element_id: u64) -> Self {
        Self {
            tile_id,
            element_id,
            _marker: PhantomData,
        }
    }

    pub fn get(&self) -> T {
        T::get_from_tiles(self.tile_id, self.element_id)
    }

    pub fn get_tile_id(&self) -> crate::rmdf::TileId {
        self.tile_id
    }

    pub fn get_element_id(&self) -> u64 {
        self.element_id
    }
}

impl<T: MapDataElement> Clone for MapDataElementRef<T> {
    fn clone(&self) -> Self {
        Self {
            tile_id: self.tile_id,
            element_id: self.element_id,
            _marker: self._marker,
        }
    }
}

impl<T: MapDataElement> PartialEq for MapDataElementRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.tile_id == other.tile_id && self.element_id == other.element_id
    }
}

impl<T: MapDataElement> Eq for MapDataElementRef<T> {}

impl<T: MapDataElement> Hash for MapDataElementRef<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tile_id.hash(state);
        self.element_id.hash(state);
    }
}

impl<T: MapDataElement + 'static> Debug for MapDataElementRef<T> {
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

    /// Test-only: Get point from test storage (used by MapDataElement trait impl)
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
                    crate::rmdf::TileId::from_coords(
                        line_record.point_b_lat,
                        line_record.point_b_lon,
                    ),
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
                    let line = line_ref.get();
                    // Determine which endpoint is not the center point
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

    fn get_or_init(tiles_path: Option<std::path::PathBuf>) -> &'static MapDataGraph {
        MAP_DATA_GRAPH.get_or_init(|| {
            let tiles_path = tiles_path.expect("tiles path must be passed in when calling init");
            let tile_manager = crate::rmdf::TileManager::new(tiles_path)
                .expect("Failed to initialize TileManager");
            MapDataGraph::new(tile_manager)
        })
    }

    #[tracing::instrument]
    pub fn init(tiles_path: std::path::PathBuf) {
        MapDataGraph::get_or_init(Some(tiles_path));
    }

    pub fn get() -> &'static MapDataGraph {
        MapDataGraph::get_or_init(None) // we've already initialized the graph
    }
}

// TODO: Rewrite tests for tile-based system
// All tests below are commented out because they rely on the old graph building functionality
// which has been removed in favor of the tile-based system.
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    /*
    use core::panic;
    use std::{collections::HashSet, u8};

    use rusty_fork::rusty_fork_test;
    use tracing::info;

    use crate::{
        router::rules::{BasicRules, GenerationRules},
        test_utils::{graph_from_test_dataset, set_graph_static, test_dataset_1},
    };

    use super::*;

    #[test]
    fn check_way_ok() {
        let map_data = MapDataGraph::new();
        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "primary".to_string(),
            )])),
        };

        assert!(map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "proposed".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "cycleway".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "hhhighway".to_string(),
                "primary".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "steps".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "pedestrian".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([("highway".to_string(), "path".to_string())])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "service".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "footway".to_string(),
            )])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([("highway".to_string(), "omg".to_string())])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "yes".to_string()),
            ])),
        };

        assert!(map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "no".to_string()),
            ])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "private".to_string()),
            ])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "yes".to_string()),
                ("service".to_string(), "yes".to_string()),
            ])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "yes".to_string()),
                ("access".to_string(), "yes".to_string()),
            ])),
        };

        assert!(map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "yes".to_string()),
                ("access".to_string(), "no".to_string()),
            ])),
        };

        assert!(!map_data.way_is_ok(&osm_way));

        let osm_way = OsmWay {
            id: 1,
            point_ids: Vec::new(),
            tags: Some(HashMap::from([
                ("highway".to_string(), "primary".to_string()),
                ("motor_vehicle".to_string(), "yes".to_string()),
                ("access".to_string(), "private".to_string()),
            ])),
        };

        assert!(!map_data.way_is_ok(&osm_way));
    }

    #[derive(Debug)]
    struct PointTest {
        lat: f32,
        lon: f32,
        lines: Vec<&'static str>,
        junction: bool,
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn check_point_consistency() {
            fn point_is_ok(map_data: &MapDataGraph, id: &u64, test: PointTest) -> bool {
                let point = map_data
                    .get_point_ref_by_id(id)
                    .unwrap_or_else(|| panic!("point {} must exist", id));
                let point = point.get();
                info!("point {:#?}", point);
                info!("test {:#?}", test);
                point.lat == test.lat
                    && point.lon == test.lon
                    && point.lines.len() == test.lines.len()
                    && point.lines.iter().enumerate().all(|(idx, l)| {
                        let test_line_id = test
                            .lines
                            .get(idx)
                            .unwrap_or_else(|| panic!("{}: line at idx {} must exist", id, idx));
                        l.get().line_id() == *test_line_id
                    })
                    && point.is_junction() == test.junction
            }
            let map_data = set_graph_static(graph_from_test_dataset(test_dataset_1()));
            assert!(point_is_ok(
                map_data,
                &1,
                PointTest {
                    lat: 1.0,
                    lon: 1.0,
                    lines: vec!["1-2"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &2,
                PointTest {
                    lat: 2.0,
                    lon: 2.0,
                    lines: vec!["1-2", "2-3"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &3,
                PointTest {
                    lat: 3.0,
                    lon: 3.0,
                    lines: vec!["2-3", "3-4", "5-3", "3-6"],
                    junction: true
                }
            ));
            assert!(point_is_ok(
                map_data,
                &4,
                PointTest {
                    lat: 4.0,
                    lon: 4.0,
                    lines: vec!["3-4", "4-8"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &5,
                PointTest {
                    lat: 5.0,
                    lon: 5.0,
                    lines: vec!["5-3"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &6,
                PointTest {
                    lat: 6.0,
                    lon: 6.0,
                    lines: vec!["3-6", "6-7", "6-8"],
                    junction: true
                }
            ));
            assert!(point_is_ok(
                map_data,
                &7,
                PointTest {
                    lat: 7.0,
                    lon: 7.0,
                    lines: vec!["6-7"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &8,
                PointTest {
                    lat: 8.0,
                    lon: 8.0,
                    lines: vec!["4-8", "8-9", "6-8"],
                    junction: true
                }
            ));
            assert!(point_is_ok(
                map_data,
                &9,
                PointTest {
                    lat: 9.0,
                    lon: 9.0,
                    lines: vec!["8-9"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &11,
                PointTest {
                    lat: 11.0,
                    lon: 11.0,
                    lines: vec!["11-12"],
                    junction: false
                }
            ));
            assert!(point_is_ok(
                map_data,
                &12,
                PointTest {
                    lat: 12.0,
                    lon: 12.0,
                    lines: vec!["11-12"],
                    junction: false
                }
            ));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn check_line_consistency() {
            fn line_is_ok(
                map_data: &MapDataGraph,
                id: &str,
                test_points: (u64, u64),
            ) -> bool {
                let line = map_data
                    .lines
                    .iter()
                    .find(|l| l.line_id() == *id)
                    .unwrap_or_else(|| panic!("line {} must exist", id));
                info!("line {:#?}", line);
                info!("test {:#?}", test_points);
                     line.points.0.get().id == test_points.0
                    && line.points.1.get().id == test_points.1
            }
            let map_data = set_graph_static(graph_from_test_dataset(test_dataset_1()));
            assert!(line_is_ok(map_data, "1-2", (1, 2)));
            assert!(line_is_ok(map_data, "2-3", (2, 3)));
            assert!(line_is_ok(map_data, "3-4", (3, 4)));
            assert!(line_is_ok(map_data, "5-3", (5, 3)));
            assert!(line_is_ok(map_data, "3-6", (3, 6)));
            assert!(line_is_ok(map_data, "6-7", (6, 7)));
            assert!(line_is_ok(map_data, "4-8", (4, 8)));
            assert!(line_is_ok(map_data, "8-9", (8, 9)));
            assert!(line_is_ok(map_data, "6-8", (6, 8)));
            assert!(line_is_ok(map_data, "11-12", (11, 12)));
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn check_missing_points() {
            let mut map_data = MapDataGraph::new();
            let res = map_data.insert_way(OsmWay {
                id: 1,
                point_ids: vec![1],
                tags:Some(HashMap::from([("highway".to_string(), "primary".to_string())]))
            });
            if res.is_ok() {
                assert!(false);
            } else if let Err(e) = res {
                if let MapDataError::MissingPoint { point_id: p } = e {
                    assert_eq!(p, 1);
                } else {
                    assert!(false);
                }
            }
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn mark_junction() {
            let map_data = set_graph_static(graph_from_test_dataset(test_dataset_1()));
            let point = map_data.get_point_ref_by_id(&5).unwrap();
            let points = map_data.get_adjacent(point);
            points.iter().for_each(|p| {
                assert!((p.1.get().id == 3 && p.1.get().is_junction()) || p.1.get().id != 3)
            });

            let point = map_data.get_point_ref_by_id(&3).unwrap();
            let points = map_data.get_adjacent(point);
            let non_junctions = [2, 5, 4];
            points.iter().for_each(|p| {
                assert!(
                    ((non_junctions.contains(&p.1.get().id) && !p.1.get().is_junction())
                        || !non_junctions.contains(&p.1.get().id))
                )
            });
            points.iter().for_each(|p| {
                assert!((p.1.get().id == 6 && p.1.get().is_junction()) || p.1.get().id != 6)
            });
        }
    }

    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn adjacent_lookup() {
            let map_data = set_graph_static(graph_from_test_dataset(test_dataset_1()));

            let tests: Vec<(u8, MapDataPointRef, Vec<(String, u64)>)> = vec![
                (
                    1,
                    MapDataGraph::get().get_point_ref_by_id(&2).unwrap(),
                    vec![(String::from("1-2"), 1), (String::from("2-3"), 3)],
                ),
                (
                    2,
                    MapDataGraph::get().get_point_ref_by_id(&3).unwrap(),
                    vec![
                        (String::from("5-3"), 5),
                        (String::from("6-3"), 6),
                        (String::from("2-3"), 2),
                        (String::from("4-3"), 4),
                    ],
                ),
                (
                    3,
                    MapDataGraph::get().get_point_ref_by_id(&1).unwrap(),
                    vec![(String::from("1-2"), 2)],
                ),
            ];

            for test in tests {
                let (_test_id, point, expected_result) = test;
                let adj_elements = map_data.get_adjacent(point);
                assert_eq!(adj_elements.len(), expected_result.len());
                for (adj_line, adj_point) in &adj_elements {
                    let adj_match = expected_result.iter().find(|&(line_id, point_id)| {
                        line_id.split("-").collect::<HashSet<_>>()
                            == adj_line.get().line_id().split("-").collect::<HashSet<_>>()
                            && point_id == &adj_point.get().id
                    });
                    assert!(adj_match.is_some());
                }
            }
        }
    }

    type ClosestTest = (Vec<OsmNode>, Vec<OsmWay>, Option<RouterRules>, OsmNode, u64);

    fn run_closest_test(test: ClosestTest) {
        let (points, ways, rules, check_point, closest_id) = test;
        let mut map_data = MapDataGraph::new();
        for point in &points {
            map_data.insert_node(point.clone());
        }
        for point in points {
            if !ways.iter().any(|w| w.point_ids.contains(&point.id)) {
                map_data
                    .insert_way(OsmWay {
                        id: point.id,
                        tags: Some(HashMap::from([(
                            "highway".to_string(),
                            "primary".to_string(),
                        )])),
                        point_ids: vec![point.id, point.id],
                    })
                    .expect("failed to insert dummy way");
            }
        }
        for way in ways {
            map_data.insert_way(way).expect("failed to insert way");
        }

        map_data.generate_point_hashes();

        let map_data = set_graph_static(map_data);

        let closest = map_data.get_closest_to_coords(
            check_point.lat as f32,
            check_point.lon as f32,
            &rules.map_or(
                RouterRules {
                    ..Default::default()
                },
                |r| r,
            ),
            false,
            None,
        );
        if let Some(closest) = closest {
            assert_eq!(closest.get().id, closest_id);
        } else {
            panic!("No points found");
        }
    }
    fn get_closest_tests() -> [ClosestTest; 6] {
        [
            (
                vec![
                    // 0
                    OsmNode {
                        id: 1,
                        lat: 57.1640,
                        lon: 24.8652,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![],
                None,
                OsmNode {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                1,
            ),
            (
                vec![
                    // 1
                    OsmNode {
                        id: 1,
                        lat: 57.1740,
                        lon: 24.8630,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        id: 2,
                        lat: 57.1640,
                        lon: 24.8652,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![],
                None,
                OsmNode {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                2,
            ),
            (
                vec![
                    // 2
                    OsmNode {
                        // 701.26 meters
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 525.74 meters
                        id: 2,
                        lat: 57.168,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 438.77 meters
                        id: 3,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![],
                None,
                OsmNode {
                    id: 0,
                    lat: 57.163429387682214,
                    lon: 24.87742424011231,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                3,
            ),
            (
                vec![
                    // 3
                    OsmNode {
                        // 2642.91 meters
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 3777.35 meters
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![],
                None,
                OsmNode {
                    id: 0,
                    lat: 57.193343289610794,
                    lon: 24.872531890869144,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                1,
            ),
            (
                vec![
                    // 4
                    OsmNode {
                        // 2642.91 meters
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 3777.35 meters
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![],
                None,
                OsmNode {
                    id: 0,
                    lat: 57.193343289610794,
                    lon: 24.872531890869144,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                1,
            ),
            (
                vec![
                    // 5
                    OsmNode {
                        // 701.26 meters
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 525.74 meters
                        id: 2,
                        lat: 57.168,
                        lon: 24.875192642211914,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                    OsmNode {
                        // 438.77 meters
                        id: 3,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                        residential_in_proximity: false,
                        nogo_area: false,
                    },
                ],
                vec![OsmWay {
                    id: 33,
                    point_ids: vec![3],
                    tags: Some(HashMap::from([(
                        "highway".to_string(),
                        "trunk".to_string(),
                    )])),
                }],
                Some(RouterRules {
                    basic: BasicRules::default(),
                    highway: Some(HashMap::from([(
                        "trunk".to_string(),
                        RulesTagValueAction::Avoid,
                    )])),
                    surface: None,
                    smoothness: None,
                    generation: GenerationRules::default(),
                }),
                OsmNode {
                    id: 0,
                    lat: 57.163429387682214,
                    lon: 24.87742424011231,
                    residential_in_proximity: false,
                    nogo_area: false,
                },
                2,
            ),
        ]
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_0() {
            let tests = get_closest_tests();
            run_closest_test(tests[0].clone());
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_1() {
            let tests = get_closest_tests();
            run_closest_test(tests[1].clone());
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_2() {
            let tests = get_closest_tests();
            run_closest_test(tests[2].clone());
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_3() {
            let tests = get_closest_tests();
            run_closest_test(tests[3].clone());
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_4() {
            let tests = get_closest_tests();
            run_closest_test(tests[4].clone());
        }
    }
    rusty_fork_test! {
        #![rusty_fork(timeout_ms = 2000)]
        #[test]
        fn closest_lookup_5() {
            let tests = get_closest_tests();
            run_closest_test(tests[5].clone());
        }
    }
    */
}
