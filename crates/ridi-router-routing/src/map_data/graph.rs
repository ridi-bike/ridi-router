use std::{
    cmp::Eq,
    fmt::{Debug, Display},
    hash::Hash,
    marker::PhantomData,
};

use super::{line::MapDataLine, point::MapDataPoint};
use crate::{
    map_data::line::LineDirection,
    rmdf::format::{TagSetRecord, TileId},
    router::rules::RouterRules,
};
use ridi_router_common::manifest::TileManifest;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use tracing::warn;

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

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
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
}

impl ElementTags {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> (usize, usize) {
        (self.tag_values.len(), self.tag_sets.len())
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

#[allow(dead_code)]
pub type MapDataLineRef = MapDataElementRef<MapDataLine>;
#[allow(dead_code)]
pub type MapDataPointRef = MapDataElementRef<MapDataPoint>;
#[allow(dead_code)]
pub(crate) const ADJACENT_INLINE_CAPACITY: usize = 8;
#[allow(dead_code)]
pub(crate) type AdjacentRef = (MapDataLineRef, MapDataPointRef);
#[allow(dead_code)]
pub(crate) type AdjacentRefs = SmallVec<[AdjacentRef; ADJACENT_INLINE_CAPACITY]>;

pub struct MapDataGraph {
    tile_manager: std::sync::RwLock<crate::rmdf::TileManager>,
    #[allow(dead_code)]
    tags: std::sync::RwLock<ElementTags>,
    // Test-only: in-memory storage for unit tests
    #[cfg(test)]
    test_points: std::sync::RwLock<std::collections::HashMap<u64, MapDataPoint>>,
    #[cfg(test)]
    test_lines: std::sync::RwLock<std::collections::HashMap<u64, MapDataLine>>,
}

#[hotpath::measure_all]
impl MapDataGraph {
    pub fn new(tile_manager: crate::rmdf::TileManager) -> Self {
        Self {
            tile_manager: std::sync::RwLock::new(tile_manager),
            tags: std::sync::RwLock::new(ElementTags::new()),
            #[cfg(test)]
            test_points: std::sync::RwLock::new(std::collections::HashMap::new()),
            #[cfg(test)]
            test_lines: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn open(tiles_dir: std::path::PathBuf, manifest: TileManifest) -> Self {
        Self::new(crate::rmdf::TileManager::from_manifest(manifest, tiles_dir))
    }

    /// Test-only: Create a MapDataGraph with dummy TileManager for unit tests
    #[cfg(test)]
    pub fn new_test() -> Self {
        // Create a minimal TileManager with an empty manifest
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
            test_points: std::sync::RwLock::new(std::collections::HashMap::new()),
            test_lines: std::sync::RwLock::new(std::collections::HashMap::new()),
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

    fn logical_line_key(point_a_osm_id: u64, point_b_osm_id: u64) -> (u64, u64) {
        (point_a_osm_id, point_b_osm_id)
    }

    fn logical_line_key_from_record(line_record: &crate::rmdf::format::LineRecord) -> (u64, u64) {
        Self::logical_line_key(line_record.point_a_osm_id, line_record.point_b_osm_id)
    }

    fn normalize_rule_line_keys(
        tile_manager: &mut crate::rmdf::TileManager,
        line_refs: &[MapDataLineRef],
        representative_line_refs: &mut std::collections::HashMap<(u64, u64), MapDataLineRef>,
    ) -> Option<Vec<(u64, u64)>> {
        let mut line_keys = Vec::with_capacity(line_refs.len());

        for line_ref in line_refs {
            let line_record = tile_manager
                .get_line_by_index(line_ref.get_tile_id(), line_ref.get_element_id() as usize)
                .ok()?;
            let line_key = Self::logical_line_key_from_record(&line_record);
            representative_line_refs
                .entry(line_key)
                .or_insert_with(|| line_ref.clone());
            line_keys.push(line_key);
        }

        line_keys.sort_unstable();
        line_keys.dedup();
        Some(line_keys)
    }

    fn get_merged_point_from_tiles(
        &self,
        tile_id: crate::rmdf::TileId,
        osm_id: u64,
    ) -> Option<(
        crate::rmdf::format::PointRecord,
        Vec<MapDataLineRef>,
        Vec<crate::map_data::rule::MapDataRule>,
    )> {
        #[cfg(test)]
        if let Some(point) = self.get_test_point(osm_id) {
            let mut flags = 0;
            if point.residential_in_proximity {
                flags |= crate::rmdf::format::PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
            }
            if point.nogo_area {
                flags |= crate::rmdf::format::PointRecord::NOGO_AREA_FLAG;
            }

            return Some((
                crate::rmdf::format::PointRecord {
                    osm_id: point.id,
                    lat: point.lat,
                    lon: point.lon,
                    lines_offset: 0,
                    lines_count: point.lines.len() as u32,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: point.rules.len() as u32,
                    flags,
                    _padding2: 0,
                },
                point.lines,
                point.rules,
            ));
        }

        let mut tile_manager = self.tile_manager.write().unwrap();
        let (source_tile_id, source_point_record) = tile_manager
            .get_point_by_id_with_neighbor_fallback(tile_id, osm_id)
            .ok()??;
        let mut point_copies = tile_manager
            .get_point_copies_for_coords(source_point_record.lat, source_point_record.lon, osm_id)
            .ok()?;
        if point_copies.is_empty() {
            point_copies.push((source_tile_id, source_point_record));
        }

        let (base_tile_id, base_point_record) = point_copies.first().copied()?;
        let mut merged_flags = 0_u16;
        let mut representative_line_refs =
            std::collections::HashMap::<(u64, u64), MapDataLineRef>::new();
        let mut line_order = Vec::<(u64, u64)>::new();

        for (copy_tile_id, point_record) in &point_copies {
            merged_flags |= point_record.flags;

            if point_record.lat != base_point_record.lat
                || point_record.lon != base_point_record.lon
                || point_record.flags != base_point_record.flags
            {
                warn!(
                    requested_tile = ?tile_id,
                    ?base_tile_id,
                    ?copy_tile_id,
                    osm_id,
                    "Point copies differ across overlap tiles; merging deterministically"
                );
            }

            let line_indices = tile_manager
                .get_line_indices_for_point(*copy_tile_id, point_record)
                .ok()?
                .to_vec();

            for line_index in line_indices {
                let line_record = tile_manager
                    .get_line_by_index(*copy_tile_id, line_index as usize)
                    .ok()?;
                let line_key = Self::logical_line_key_from_record(&line_record);
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    representative_line_refs.entry(line_key)
                {
                    entry.insert(MapDataLineRef::new(*copy_tile_id, line_index));
                    line_order.push(line_key);
                }
            }
        }

        let lines = line_order
            .iter()
            .filter_map(|line_key| representative_line_refs.get(line_key).cloned())
            .collect::<Vec<_>>();

        let mut rules = Vec::new();
        let mut seen_rules =
            std::collections::HashSet::<(u8, Vec<(u64, u64)>, Vec<(u64, u64)>)>::new();

        for (copy_tile_id, point_record) in &point_copies {
            let copy_rules = tile_manager
                .get_rules_for_point(*copy_tile_id, point_record)
                .ok()?;

            for rule in copy_rules {
                let Some(from_line_keys) = Self::normalize_rule_line_keys(
                    &mut tile_manager,
                    &rule.from_lines,
                    &mut representative_line_refs,
                ) else {
                    warn!(
                        requested_tile = ?tile_id,
                        ?copy_tile_id,
                        osm_id,
                        "Skipping overlap-merged rule with unreadable from-line refs"
                    );
                    continue;
                };
                let Some(to_line_keys) = Self::normalize_rule_line_keys(
                    &mut tile_manager,
                    &rule.to_lines,
                    &mut representative_line_refs,
                ) else {
                    warn!(
                        requested_tile = ?tile_id,
                        ?copy_tile_id,
                        osm_id,
                        "Skipping overlap-merged rule with unreadable to-line refs"
                    );
                    continue;
                };

                let rule_kind = match rule.rule_type {
                    crate::map_data::rule::MapDataRuleType::OnlyAllowed => 0,
                    crate::map_data::rule::MapDataRuleType::NotAllowed => 1,
                };
                if !seen_rules.insert((rule_kind, from_line_keys.clone(), to_line_keys.clone())) {
                    continue;
                }

                let from_lines = from_line_keys
                    .iter()
                    .filter_map(|line_key| representative_line_refs.get(line_key).cloned())
                    .collect::<Vec<_>>();
                let to_lines = to_line_keys
                    .iter()
                    .filter_map(|line_key| representative_line_refs.get(line_key).cloned())
                    .collect::<Vec<_>>();

                if from_lines.len() != from_line_keys.len() || to_lines.len() != to_line_keys.len()
                {
                    warn!(
                        requested_tile = ?tile_id,
                        ?copy_tile_id,
                        osm_id,
                        "Skipping overlap-merged rule after representative registration failed"
                    );
                    continue;
                }

                rules.push(crate::map_data::rule::MapDataRule {
                    from_lines,
                    to_lines,
                    rule_type: rule.rule_type,
                });
            }
        }

        Some((
            crate::rmdf::format::PointRecord {
                osm_id: base_point_record.osm_id,
                lat: base_point_record.lat,
                lon: base_point_record.lon,
                lines_offset: 0,
                lines_count: lines.len() as u32,
                _padding1: 0,
                rules_offset: 0,
                rules_count: rules.len() as u32,
                flags: merged_flags,
                _padding2: 0,
            },
            lines,
            rules,
        ))
    }

    // Get point data from tiles (or test storage in test mode)
    pub fn get_point_from_tiles(&self, tile_id: crate::rmdf::TileId, osm_id: u64) -> MapDataPoint {
        if let Some((point_record, lines, rules)) =
            self.get_merged_point_from_tiles(tile_id, osm_id)
        {
            return MapDataPoint {
                id: point_record.osm_id,
                lat: point_record.lat,
                lon: point_record.lon,
                lines,
                rules,
                residential_in_proximity: point_record.residential_in_proximity(),
                nogo_area: point_record.nogo_area(),
            };
        }

        unreachable!("point should exist after tile-backed point lookup")
    }

    pub(crate) fn get_point_record_from_tiles(
        &self,
        tile_id: crate::rmdf::TileId,
        osm_id: u64,
    ) -> Option<crate::rmdf::format::PointRecord> {
        self.get_merged_point_from_tiles(tile_id, osm_id)
            .map(|(point_record, _, _)| point_record)
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

        {
            let tm = self.tile_manager.read().unwrap();
            if let Some(line_record) = tm
                .get_line_by_index_if_loaded(tile_id, line_index)
                .expect("Failed to get line from loaded tile")
            {
                return MapDataLine {
                    points: (
                        MapDataPointRef::new(
                            tm.tile_id_for_coords(line_record.point_a_lat, line_record.point_a_lon),
                            line_record.point_a_osm_id,
                        ),
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
                };
            }
        }

        let mut tm = self.tile_manager.write().unwrap();
        let line_record = tm
            .get_line_by_index(tile_id, line_index)
            .expect("Failed to get line from tile");
        MapDataLine {
            points: (
                MapDataPointRef::new(
                    tm.tile_id_for_coords(line_record.point_a_lat, line_record.point_a_lon),
                    line_record.point_a_osm_id,
                ),
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

        if let Some(value) = self
            .tile_manager
            .read()
            .unwrap()
            .get_tag_value_if_loaded(tag_value_ref.tile_id, tag_value_ref.tag_value_idx)
            .ok()
            .flatten()
        {
            return Some(value);
        }

        self.tile_manager
            .write()
            .unwrap()
            .get_tag_value(tag_value_ref.tile_id, tag_value_ref.tag_value_idx)
            .ok()
    }

    pub fn get_tag_set(&self, tag_set_ref: &ElementTagSetRef) -> ElementTagSet {
        let loaded_tag_set_record = {
            let tile_manager = self.tile_manager.read().unwrap();
            tile_manager
                .get_tag_set_record_if_loaded(tag_set_ref.tile_id, tag_set_ref.tag_set_idx)
                .ok()
                .flatten()
        };
        let tag_set_record = loaded_tag_set_record
            .or_else(|| {
                self.tile_manager
                    .write()
                    .unwrap()
                    .get_tag_set_record(tag_set_ref.tile_id, tag_set_ref.tag_set_idx)
                    .ok()
            })
            .unwrap_or(TagSetRecord {
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

        let point =
            self.get_point_from_tiles(center_point.get_tile_id(), center_point.get_element_id());
        let mut adjacent = Vec::with_capacity(point.lines.len());

        for line_ref in point.lines {
            let line = self
                .get_line_from_tiles(line_ref.get_tile_id(), line_ref.get_element_id() as usize);
            let Some(other_point) =
                (if line.points.0.get_element_id() == center_point.get_element_id() {
                    Some(line.points.1.clone())
                } else if line.points.1.get_element_id() == center_point.get_element_id() {
                    Some(line.points.0.clone())
                } else {
                    None
                })
            else {
                warn!(
                    ?center_point,
                    ?line_ref,
                    "Merged adjacent line no longer references the requested point"
                );
                continue;
            };

            if other_point.get_tile_id() != center_point.get_tile_id() {
                let other_tile_available = self
                    .tile_manager
                    .read()
                    .unwrap()
                    .is_tile_available(other_point.get_tile_id());
                if !other_tile_available
                    || self
                        .get_point_record_from_tiles(
                            other_point.get_tile_id(),
                            other_point.get_element_id(),
                        )
                        .is_none()
                {
                    continue;
                }
            }

            adjacent.push((line_ref, other_point));
        }

        adjacent
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

        let result = match tm.get_closest_to_coords(
            lat,
            lon,
            rules,
            avoid_proximity_to_residential,
            _limit_to_hw_tags,
        ) {
            Ok(Some(result)) => result,
            Ok(None) => return None,
            Err(error) => {
                warn!(lat, lon, error = ?error, "Closest-point lookup failed");
                return None;
            }
        };

        Some(MapDataPointRef::new(result.0, result.1))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, thread};

    use super::*;
    use crate::map_data::rule::MapDataRuleType;
    use ridi_router_test_support::rmdf::{
        create_missing_neighbor_fixture, empty_neighbors, manifest_bounds, unique_test_dir,
        write_manifest, write_tile, LineRecord, PointRecord, RuleRecord, TileId, TileManifest,
        TileMetadata, TileNeighbors, TileSpec, SYNTHETIC_TILE_BOUNDS, SYNTHETIC_TILE_ID,
        SYNTHETIC_TILE_SIZE_DEGREES,
    };

    #[test]
    fn test_get_point_from_tiles_hydrates_rules() {
        let fixture = create_rule_fixture("map-data-graph-rule-hydration");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let point = graph.get_point_from_tiles(fixture.tile_id, fixture.via_osm_id);

        assert_eq!(point.id, fixture.via_osm_id);
        assert_eq!(
            point.lines,
            vec![
                MapDataLineRef::new(fixture.tile_id, 0),
                MapDataLineRef::new(fixture.tile_id, 1),
                MapDataLineRef::new(fixture.tile_id, 2),
            ]
        );
        assert_eq!(point.rules.len(), 2);

        assert_eq!(point.rules[0].rule_type, MapDataRuleType::OnlyAllowed);
        assert_eq!(
            point.rules[0].from_lines,
            vec![MapDataLineRef::new(fixture.tile_id, 0)]
        );
        assert_eq!(
            point.rules[0].to_lines,
            vec![MapDataLineRef::new(fixture.tile_id, 1)]
        );

        assert_eq!(point.rules[1].rule_type, MapDataRuleType::NotAllowed);
        assert_eq!(
            point.rules[1].from_lines,
            vec![MapDataLineRef::new(fixture.tile_id, 0)]
        );
        assert_eq!(
            point.rules[1].to_lines,
            vec![MapDataLineRef::new(fixture.tile_id, 2)]
        );

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_keeps_points_without_rules() {
        let fixture = create_rule_fixture("map-data-graph-no-rules");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let point = graph.get_point_from_tiles(fixture.tile_id, fixture.to_osm_id);

        assert_eq!(point.id, fixture.to_osm_id);
        assert!(point.rules.is_empty());
        assert_eq!(point.lines, vec![MapDataLineRef::new(fixture.tile_id, 1)]);

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_keeps_points_without_lines() {
        let fixture = create_rule_fixture("map-data-graph-no-lines");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let point = graph.get_point_from_tiles(fixture.tile_id, fixture.isolated_osm_id);

        assert_eq!(point.id, fixture.isolated_osm_id);
        assert!(point.lines.is_empty());
        assert!(point.rules.is_empty());

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_allows_concurrent_loaded_tile_hits() {
        let fixture = create_rule_fixture("map-data-graph-concurrent-loaded-point-hits");
        let graph = Arc::new(MapDataGraph::new(
            crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap(),
        ));

        let warmed = graph.get_point_from_tiles(fixture.tile_id, fixture.via_osm_id);
        assert_eq!(warmed.id, fixture.via_osm_id);

        let mut workers = Vec::new();
        for _ in 0..8 {
            let graph = Arc::clone(&graph);
            let tile_id = fixture.tile_id;
            let osm_id = fixture.via_osm_id;
            workers.push(thread::spawn(move || {
                for _ in 0..100 {
                    let point = graph.get_point_from_tiles(tile_id, osm_id);
                    assert_eq!(point.id, osm_id);
                    assert_eq!(point.lines.len(), 3);
                    assert_eq!(point.rules.len(), 2);
                }
            }));
        }

        for worker in workers {
            worker.join().unwrap();
        }

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_keeps_cross_tile_behavior() {
        let fixture = create_cross_tile_fixture("map-data-graph-cross-tile-adjacent");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let adjacent =
            graph.get_adjacent(MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id));

        assert_eq!(adjacent.len(), 2);
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_a, 0),
            MapDataPointRef::new(fixture.tile_a, fixture.in_tile_neighbor_osm_id),
        )));
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_a, 1),
            MapDataPointRef::new(fixture.tile_b, fixture.cross_tile_neighbor_osm_id),
        )));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_uses_loaded_only_fast_path_for_warm_same_tile_lookup() {
        let fixture = create_rule_fixture("map-data-graph-warm-same-tile-adjacent");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let warmed = graph.get_point_from_tiles(fixture.tile_id, fixture.via_osm_id);
        assert_eq!(warmed.id, fixture.via_osm_id);

        let adjacent =
            graph.get_adjacent(MapDataPointRef::new(fixture.tile_id, fixture.via_osm_id));

        assert_eq!(adjacent.len(), 3);
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_id, 0),
            MapDataPointRef::new(fixture.tile_id, 2000),
        )));
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_id, 1),
            MapDataPointRef::new(fixture.tile_id, fixture.to_osm_id),
        )));
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_id, 2),
            MapDataPointRef::new(fixture.tile_id, 2003),
        )));
        assert_eq!(graph.tile_manager.read().unwrap().loaded_tile_count(), 1);

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_uses_loaded_only_fast_path_for_warm_cross_tile_lookup() {
        let fixture = create_cross_tile_fixture("map-data-graph-warm-cross-tile-adjacent");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let warmed_a = graph.get_point_from_tiles(fixture.tile_a, fixture.center_osm_id);
        assert_eq!(warmed_a.id, fixture.center_osm_id);
        let warmed_b =
            graph.get_point_from_tiles(fixture.tile_b, fixture.cross_tile_neighbor_osm_id);
        assert_eq!(warmed_b.id, fixture.cross_tile_neighbor_osm_id);

        let adjacent =
            graph.get_adjacent(MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id));

        assert_eq!(adjacent.len(), 2);
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_a, 0),
            MapDataPointRef::new(fixture.tile_a, fixture.in_tile_neighbor_osm_id),
        )));
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_a, 1),
            MapDataPointRef::new(fixture.tile_b, fixture.cross_tile_neighbor_osm_id),
        )));
        assert_eq!(graph.tile_manager.read().unwrap().loaded_tile_count(), 2);

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_filters_missing_neighbor_edges_without_failing() {
        let fixture = create_missing_neighbor_fixture("map-data-graph-missing-neighbor");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let adjacent =
            graph.get_adjacent(MapDataPointRef::new(fixture.tile_a, fixture.center_osm_id));

        assert_eq!(adjacent.len(), 1);
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_a, 0),
            MapDataPointRef::new(fixture.tile_a, fixture.in_tile_neighbor_osm_id),
        )));
        assert!(!adjacent.iter().any(|(_, other_point_ref)| {
            other_point_ref.get_tile_id() == fixture.missing_tile
                || other_point_ref.get_element_id() == fixture.missing_neighbor_osm_id
        }));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    struct RuleFixture {
        dir: std::path::PathBuf,
        tile_id: TileId,
        via_osm_id: u64,
        to_osm_id: u64,
        isolated_osm_id: u64,
    }

    struct CrossTileFixture {
        dir: std::path::PathBuf,
        tile_a: TileId,
        tile_b: TileId,
        center_osm_id: u64,
        in_tile_neighbor_osm_id: u64,
        cross_tile_neighbor_osm_id: u64,
    }

    fn create_rule_fixture(prefix: &str) -> RuleFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_id = SYNTHETIC_TILE_ID;
        let via_osm_id = 2001;
        let to_osm_id = 2002;
        let side_osm_id = 2003;
        let isolated_osm_id = 2004;

        let points = vec![
            PointRecord {
                osm_id: 2000,
                lat: 10.10,
                lon: 20.10,
                lines_offset: 0,
                lines_count: 1,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: via_osm_id,
                lat: 10.11,
                lon: 20.10,
                lines_offset: 1,
                lines_count: 3,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 2,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: to_osm_id,
                lat: 10.12,
                lon: 20.10,
                lines_offset: 4,
                lines_count: 1,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: side_osm_id,
                lat: 10.11,
                lon: 20.11,
                lines_offset: 5,
                lines_count: 1,
                _padding1: 0,
                rules_offset: 0,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
            PointRecord {
                osm_id: isolated_osm_id,
                lat: 10.13,
                lon: 20.12,
                lines_offset: 6,
                lines_count: 0,
                _padding1: 0,
                rules_offset: 2,
                rules_count: 0,
                flags: 0,
                _padding2: 0,
            },
        ];

        let lines = vec![
            LineRecord {
                point_a_osm_id: 2000,
                point_a_lat: 10.10,
                point_a_lon: 20.10,
                point_b_osm_id: via_osm_id,
                point_b_lat: 10.11,
                point_b_lon: 20.10,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: via_osm_id,
                point_a_lat: 10.11,
                point_a_lon: 20.10,
                point_b_osm_id: to_osm_id,
                point_b_lat: 10.12,
                point_b_lon: 20.10,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: via_osm_id,
                point_a_lat: 10.11,
                point_a_lon: 20.10,
                point_b_osm_id: side_osm_id,
                point_b_lat: 10.11,
                point_b_lon: 20.11,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
        ];

        let rule_line_refs = vec![0, 1, 0, 2];
        let rules = vec![
            RuleRecord {
                from_lines_offset: 0,
                from_lines_count: 1,
                _padding1: 0,
                to_lines_offset: 1,
                to_lines_count: 1,
                rule_type: 0,
                _padding2: 0,
                _padding3: 0,
            },
            RuleRecord {
                from_lines_offset: 2,
                from_lines_count: 1,
                _padding1: 0,
                to_lines_offset: 3,
                to_lines_count: 1,
                rule_type: 1,
                _padding2: 0,
                _padding3: 0,
            },
        ];

        let tile_path = write_tile(
            &dir,
            &TileSpec {
                tile_id,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points,
                lines,
                line_refs: vec![0, 0, 1, 2, 1, 2],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules,
                rule_line_refs,
            },
        );

        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: SYNTHETIC_TILE_SIZE_DEGREES,
                format_version: 1,
                generated_at: "2026-04-05T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![TileMetadata {
                    filename: tile_id.to_filename(),
                    col: tile_id.col,
                    row: tile_id.row,
                    bounds: manifest_bounds(SYNTHETIC_TILE_BOUNDS),
                    neighbors: empty_neighbors(),
                    size_bytes: fs::metadata(&tile_path).unwrap().len(),
                    point_count: 5,
                    line_count: 3,
                    checksum: "sha256:test".to_string(),
                    military_geojson_filename: None,
                }],
            },
        );

        RuleFixture {
            dir,
            tile_id,
            via_osm_id,
            to_osm_id,
            isolated_osm_id,
        }
    }

    fn create_cross_tile_fixture(prefix: &str) -> CrossTileFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let center_osm_id = 20_000;
        let in_tile_neighbor_osm_id = 20_001;
        let cross_tile_neighbor_osm_id = 20_002;

        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: vec![
                    PointRecord {
                        osm_id: center_osm_id,
                        lat: 10.50,
                        lon: 20.95,
                        lines_offset: 0,
                        lines_count: 2,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: in_tile_neighbor_osm_id,
                        lat: 10.60,
                        lon: 20.80,
                        lines_offset: 2,
                        lines_count: 1,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![
                    LineRecord {
                        point_a_osm_id: center_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.95,
                        point_b_osm_id: in_tile_neighbor_osm_id,
                        point_b_lat: 10.60,
                        point_b_lon: 20.80,
                        direction: 0,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                    LineRecord {
                        point_a_osm_id: center_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.95,
                        point_b_osm_id: cross_tile_neighbor_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 21.05,
                        direction: 0,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                ],
                line_refs: vec![0, 1, 0],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![PointRecord {
                    osm_id: cross_tile_neighbor_osm_id,
                    lat: 10.50,
                    lon: 21.05,
                    lines_offset: 0,
                    lines_count: 0,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: 0,
                    flags: 0,
                    _padding2: 0,
                }],
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_a_filename = tile_a.to_filename();
        let tile_b_filename = tile_b.to_filename();
        write_manifest(
            &dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: 1.0,
                format_version: 1,
                generated_at: "2026-04-05T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![
                    TileMetadata {
                        filename: tile_a_filename.clone(),
                        col: tile_a.col,
                        row: tile_a.row,
                        bounds: manifest_bounds(bounds_a),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: Some(tile_b_filename.clone()),
                            west: None,
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: fs::metadata(&tile_a_path).unwrap().len(),
                        point_count: 2,
                        line_count: 2,
                        checksum: "sha256:graph-cross-a".to_string(),
                        military_geojson_filename: None,
                    },
                    TileMetadata {
                        filename: tile_b_filename,
                        col: tile_b.col,
                        row: tile_b.row,
                        bounds: manifest_bounds(bounds_b),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: None,
                            west: Some(tile_a_filename),
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: fs::metadata(&tile_b_path).unwrap().len(),
                        point_count: 1,
                        line_count: 0,
                        checksum: "sha256:graph-cross-b".to_string(),
                        military_geojson_filename: None,
                    },
                ],
            },
        );

        CrossTileFixture {
            dir,
            tile_a,
            tile_b,
            center_osm_id,
            in_tile_neighbor_osm_id,
            cross_tile_neighbor_osm_id,
        }
    }

    struct OverlapFixture {
        dir: std::path::PathBuf,
        tile_a: TileId,
        tile_b: TileId,
        boundary_osm_id: u64,
        left_osm_id: u64,
        right_osm_id: u64,
    }

    struct OverlapManifestTile<'a> {
        id: TileId,
        bounds: ridi_router_common::format::TileBounds,
        path: &'a std::path::Path,
        point_count: u64,
        line_count: u64,
    }

    fn write_overlap_manifest(
        dir: &std::path::Path,
        tile_a: OverlapManifestTile<'_>,
        tile_b: OverlapManifestTile<'_>,
    ) {
        let tile_a_filename = tile_a.id.to_filename();
        let tile_b_filename = tile_b.id.to_filename();
        write_manifest(
            dir,
            &TileManifest {
                version: "test".to_string(),
                tile_size_degrees: 1.0,
                format_version: 1,
                generated_at: "2026-04-05T00:00:00Z".to_string(),
                source_files: vec!["synthetic".to_string()],
                tiles: vec![
                    TileMetadata {
                        filename: tile_a_filename.clone(),
                        col: tile_a.id.col,
                        row: tile_a.id.row,
                        bounds: manifest_bounds(tile_a.bounds),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: Some(tile_b_filename.clone()),
                            west: None,
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: fs::metadata(tile_a.path).unwrap().len(),
                        point_count: tile_a.point_count,
                        line_count: tile_a.line_count,
                        checksum: "sha256:graph-overlap-a".to_string(),
                        military_geojson_filename: None,
                    },
                    TileMetadata {
                        filename: tile_b_filename,
                        col: tile_b.id.col,
                        row: tile_b.id.row,
                        bounds: manifest_bounds(tile_b.bounds),
                        neighbors: TileNeighbors {
                            north: None,
                            south: None,
                            east: None,
                            west: Some(tile_a_filename),
                            northeast: None,
                            northwest: None,
                            southeast: None,
                            southwest: None,
                        },
                        size_bytes: fs::metadata(tile_b.path).unwrap().len(),
                        point_count: tile_b.point_count,
                        line_count: tile_b.line_count,
                        checksum: "sha256:graph-overlap-b".to_string(),
                        military_geojson_filename: None,
                    },
                ],
            },
        );
    }

    fn create_overlap_missing_canonical_fixture(prefix: &str) -> OverlapFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let boundary_osm_id = 30_000;
        let left_osm_id = 30_001;
        let right_osm_id = 30_002;

        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: vec![PointRecord {
                    osm_id: left_osm_id,
                    lat: 10.50,
                    lon: 20.996,
                    lines_offset: 0,
                    lines_count: 0,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: 0,
                    flags: 0,
                    _padding2: 0,
                }],
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![
                    PointRecord {
                        osm_id: boundary_osm_id,
                        lat: 10.50,
                        lon: 20.998,
                        lines_offset: 0,
                        lines_count: 2,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: right_osm_id,
                        lat: 10.50,
                        lon: 21.002,
                        lines_offset: 2,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![
                    LineRecord {
                        point_a_osm_id: boundary_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.998,
                        point_b_osm_id: left_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 20.996,
                        direction: 0,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                    LineRecord {
                        point_a_osm_id: boundary_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.998,
                        point_b_osm_id: right_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 21.002,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                ],
                line_refs: vec![0, 1],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        write_overlap_manifest(
            &dir,
            OverlapManifestTile {
                id: tile_a,
                bounds: bounds_a,
                path: &tile_a_path,
                point_count: 1,
                line_count: 0,
            },
            OverlapManifestTile {
                id: tile_b,
                bounds: bounds_b,
                path: &tile_b_path,
                point_count: 2,
                line_count: 2,
            },
        );

        OverlapFixture {
            dir,
            tile_a,
            tile_b,
            boundary_osm_id,
            left_osm_id,
            right_osm_id,
        }
    }

    fn create_overlap_dedupe_fixture(prefix: &str) -> OverlapFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let boundary_osm_id = 31_000;
        let left_osm_id = 31_001;
        let right_osm_id = 31_002;

        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };

        let duplicated_points = vec![PointRecord {
            osm_id: boundary_osm_id,
            lat: 10.50,
            lon: 20.998,
            lines_offset: 0,
            lines_count: 2,
            _padding1: 0,
            rules_offset: 0,
            rules_count: 1,
            flags: 0,
            _padding2: 0,
        }];
        let duplicated_lines = vec![
            LineRecord {
                point_a_osm_id: boundary_osm_id,
                point_a_lat: 10.50,
                point_a_lon: 20.998,
                point_b_osm_id: left_osm_id,
                point_b_lat: 10.50,
                point_b_lon: 20.996,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
            LineRecord {
                point_a_osm_id: boundary_osm_id,
                point_a_lat: 10.50,
                point_a_lon: 20.998,
                point_b_osm_id: right_osm_id,
                point_b_lat: 10.50,
                point_b_lon: 21.002,
                direction: 0,
                _padding1: 0,
                _padding2: 0,
                tag_set_index: 0,
            },
        ];
        let duplicated_rules = vec![RuleRecord {
            from_lines_offset: 0,
            from_lines_count: 1,
            _padding1: 0,
            to_lines_offset: 1,
            to_lines_count: 1,
            rule_type: 0,
            _padding2: 0,
            _padding3: 0,
        }];
        let duplicated_rule_line_refs = vec![0, 1];

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: duplicated_points.clone(),
                lines: duplicated_lines.clone(),
                line_refs: vec![0, 1],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: duplicated_rules.clone(),
                rule_line_refs: duplicated_rule_line_refs.clone(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: duplicated_points,
                lines: duplicated_lines,
                line_refs: vec![0, 1],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: duplicated_rules,
                rule_line_refs: duplicated_rule_line_refs,
            },
        );

        write_overlap_manifest(
            &dir,
            OverlapManifestTile {
                id: tile_a,
                bounds: bounds_a,
                path: &tile_a_path,
                point_count: 1,
                line_count: 2,
            },
            OverlapManifestTile {
                id: tile_b,
                bounds: bounds_b,
                path: &tile_b_path,
                point_count: 1,
                line_count: 2,
            },
        );

        OverlapFixture {
            dir,
            tile_a,
            tile_b,
            boundary_osm_id,
            left_osm_id,
            right_osm_id,
        }
    }

    #[test]
    fn test_get_point_from_tiles_falls_back_to_overlap_copy_when_canonical_missing() {
        let fixture =
            create_overlap_missing_canonical_fixture("map-data-graph-overlap-missing-canonical");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let point = graph.get_point_from_tiles(fixture.tile_a, fixture.boundary_osm_id);
        assert_eq!(point.id, fixture.boundary_osm_id);
        assert_eq!(
            point.lines,
            vec![
                MapDataLineRef::new(fixture.tile_b, 0),
                MapDataLineRef::new(fixture.tile_b, 1),
            ]
        );

        let boundary_line = graph.get_line_from_tiles(fixture.tile_b, 0);
        assert_eq!(
            boundary_line.points.0,
            MapDataPointRef::new(fixture.tile_a, fixture.boundary_osm_id)
        );

        let adjacent = graph.get_adjacent(MapDataPointRef::new(
            fixture.tile_a,
            fixture.boundary_osm_id,
        ));
        assert_eq!(adjacent.len(), 2);
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_b, 0),
            MapDataPointRef::new(fixture.tile_a, fixture.left_osm_id),
        )));
        assert!(adjacent.contains(&(
            MapDataLineRef::new(fixture.tile_b, 1),
            MapDataPointRef::new(fixture.tile_b, fixture.right_osm_id),
        )));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_dedupes_overlap_lines_and_rules() {
        let fixture = create_overlap_dedupe_fixture("map-data-graph-overlap-dedupe-lines-rules");
        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(fixture.dir.clone()).unwrap());

        let point = graph.get_point_from_tiles(fixture.tile_a, fixture.boundary_osm_id);
        assert_eq!(
            point.lines,
            vec![
                MapDataLineRef::new(fixture.tile_a, 0),
                MapDataLineRef::new(fixture.tile_a, 1),
            ]
        );
        assert_eq!(point.rules.len(), 1);
        assert_eq!(point.rules[0].rule_type, MapDataRuleType::OnlyAllowed);
        assert_eq!(
            point.rules[0].from_lines,
            vec![MapDataLineRef::new(fixture.tile_a, 0)]
        );
        assert_eq!(
            point.rules[0].to_lines,
            vec![MapDataLineRef::new(fixture.tile_a, 1)]
        );

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_keeps_opposite_direction_overlap_lines_distinct() {
        let dir = unique_test_dir("map-data-graph-overlap-directional-lines");
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };
        let boundary_osm_id = 32_000;
        let incoming_osm_id = 32_001;
        let outgoing_osm_id = 32_002;

        let duplicated_point = PointRecord {
            osm_id: boundary_osm_id,
            lat: 10.50,
            lon: 20.998,
            lines_offset: 0,
            lines_count: 1,
            _padding1: 0,
            rules_offset: 0,
            rules_count: 0,
            flags: 0,
            _padding2: 0,
        };

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: vec![
                    duplicated_point,
                    PointRecord {
                        osm_id: outgoing_osm_id,
                        lat: 10.50,
                        lon: 20.999,
                        lines_offset: 1,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![LineRecord {
                    point_a_osm_id: boundary_osm_id,
                    point_a_lat: 10.50,
                    point_a_lon: 20.998,
                    point_b_osm_id: outgoing_osm_id,
                    point_b_lat: 10.50,
                    point_b_lon: 20.999,
                    direction: 1,
                    _padding1: 0,
                    _padding2: 0,
                    tag_set_index: 0,
                }],
                line_refs: vec![0],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![
                    duplicated_point,
                    PointRecord {
                        osm_id: incoming_osm_id,
                        lat: 10.50,
                        lon: 20.996,
                        lines_offset: 1,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![LineRecord {
                    point_a_osm_id: incoming_osm_id,
                    point_a_lat: 10.50,
                    point_a_lon: 20.996,
                    point_b_osm_id: boundary_osm_id,
                    point_b_lat: 10.50,
                    point_b_lon: 20.998,
                    direction: 1,
                    _padding1: 0,
                    _padding2: 0,
                    tag_set_index: 0,
                }],
                line_refs: vec![0],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        write_overlap_manifest(
            &dir,
            OverlapManifestTile {
                id: tile_a,
                bounds: bounds_a,
                path: &tile_a_path,
                point_count: 2,
                line_count: 1,
            },
            OverlapManifestTile {
                id: tile_b,
                bounds: bounds_b,
                path: &tile_b_path,
                point_count: 2,
                line_count: 1,
            },
        );

        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(dir.clone()).unwrap());
        let point = graph.get_point_from_tiles(tile_a, boundary_osm_id);

        assert_eq!(
            point.lines,
            vec![
                MapDataLineRef::new(tile_a, 0),
                MapDataLineRef::new(tile_b, 0),
            ]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_merges_split_overlap_rules() {
        let dir = unique_test_dir("map-data-graph-overlap-split-rules");
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };
        let boundary_osm_id = 33_000;
        let from_osm_id = 33_001;
        let left_to_osm_id = 33_002;
        let right_to_osm_id = 33_003;

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: vec![
                    PointRecord {
                        osm_id: boundary_osm_id,
                        lat: 10.50,
                        lon: 20.998,
                        lines_offset: 0,
                        lines_count: 2,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 1,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: from_osm_id,
                        lat: 10.50,
                        lon: 20.996,
                        lines_offset: 2,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: left_to_osm_id,
                        lat: 10.60,
                        lon: 20.999,
                        lines_offset: 2,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![
                    LineRecord {
                        point_a_osm_id: from_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.996,
                        point_b_osm_id: boundary_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 20.998,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                    LineRecord {
                        point_a_osm_id: boundary_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.998,
                        point_b_osm_id: left_to_osm_id,
                        point_b_lat: 10.60,
                        point_b_lon: 20.999,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                ],
                line_refs: vec![0, 1],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: vec![RuleRecord {
                    from_lines_offset: 0,
                    from_lines_count: 1,
                    _padding1: 0,
                    to_lines_offset: 1,
                    to_lines_count: 1,
                    rule_type: 1,
                    _padding2: 0,
                    _padding3: 0,
                }],
                rule_line_refs: vec![0, 1],
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![
                    PointRecord {
                        osm_id: boundary_osm_id,
                        lat: 10.50,
                        lon: 20.998,
                        lines_offset: 0,
                        lines_count: 2,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 1,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: from_osm_id,
                        lat: 10.50,
                        lon: 20.996,
                        lines_offset: 2,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: right_to_osm_id,
                        lat: 10.40,
                        lon: 21.002,
                        lines_offset: 2,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![
                    LineRecord {
                        point_a_osm_id: from_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.996,
                        point_b_osm_id: boundary_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 20.998,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                    LineRecord {
                        point_a_osm_id: boundary_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.998,
                        point_b_osm_id: right_to_osm_id,
                        point_b_lat: 10.40,
                        point_b_lon: 21.002,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                ],
                line_refs: vec![0, 1],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: vec![RuleRecord {
                    from_lines_offset: 0,
                    from_lines_count: 1,
                    _padding1: 0,
                    to_lines_offset: 1,
                    to_lines_count: 1,
                    rule_type: 0,
                    _padding2: 0,
                    _padding3: 0,
                }],
                rule_line_refs: vec![0, 1],
            },
        );

        write_overlap_manifest(
            &dir,
            OverlapManifestTile {
                id: tile_a,
                bounds: bounds_a,
                path: &tile_a_path,
                point_count: 3,
                line_count: 2,
            },
            OverlapManifestTile {
                id: tile_b,
                bounds: bounds_b,
                path: &tile_b_path,
                point_count: 3,
                line_count: 2,
            },
        );

        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(dir.clone()).unwrap());
        let point = graph.get_point_from_tiles(tile_a, boundary_osm_id);

        assert_eq!(
            point.lines,
            vec![
                MapDataLineRef::new(tile_a, 0),
                MapDataLineRef::new(tile_a, 1),
                MapDataLineRef::new(tile_b, 1),
            ]
        );
        assert_eq!(point.rules.len(), 2);
        assert!(point.rules.iter().any(|rule| {
            rule.rule_type == MapDataRuleType::NotAllowed
                && rule.from_lines == vec![MapDataLineRef::new(tile_a, 0)]
                && rule.to_lines == vec![MapDataLineRef::new(tile_a, 1)]
        }));
        assert!(point.rules.iter().any(|rule| {
            rule.rule_type == MapDataRuleType::OnlyAllowed
                && rule.from_lines == vec![MapDataLineRef::new(tile_a, 0)]
                && rule.to_lines == vec![MapDataLineRef::new(tile_b, 1)]
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_point_from_tiles_keeps_rule_when_representative_line_only_appears_in_rule_refs() {
        let dir = unique_test_dir("map-data-graph-overlap-rule-only-representative");
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let bounds_a = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 20.0,
            lon_max: 21.0,
        };
        let bounds_b = ridi_router_common::format::TileBounds {
            lat_min: 10.0,
            lat_max: 11.0,
            lon_min: 21.0,
            lon_max: 22.0,
        };
        let boundary_osm_id = 34_000;
        let from_osm_id = 34_001;
        let hidden_to_osm_id = 34_002;

        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: vec![PointRecord {
                    osm_id: boundary_osm_id,
                    lat: 10.50,
                    lon: 20.998,
                    lines_offset: 0,
                    lines_count: 0,
                    _padding1: 0,
                    rules_offset: 0,
                    rules_count: 0,
                    flags: 0,
                    _padding2: 0,
                }],
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_b,
                bounds: bounds_b,
                spatial_index: Vec::new(),
                points: vec![
                    PointRecord {
                        osm_id: boundary_osm_id,
                        lat: 10.50,
                        lon: 20.998,
                        lines_offset: 0,
                        lines_count: 1,
                        _padding1: 0,
                        rules_offset: 0,
                        rules_count: 1,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: from_osm_id,
                        lat: 10.50,
                        lon: 20.996,
                        lines_offset: 1,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                    PointRecord {
                        osm_id: hidden_to_osm_id,
                        lat: 10.50,
                        lon: 21.002,
                        lines_offset: 1,
                        lines_count: 0,
                        _padding1: 0,
                        rules_offset: 1,
                        rules_count: 0,
                        flags: 0,
                        _padding2: 0,
                    },
                ],
                lines: vec![
                    LineRecord {
                        point_a_osm_id: from_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.996,
                        point_b_osm_id: boundary_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 20.998,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                    LineRecord {
                        point_a_osm_id: boundary_osm_id,
                        point_a_lat: 10.50,
                        point_a_lon: 20.998,
                        point_b_osm_id: hidden_to_osm_id,
                        point_b_lat: 10.50,
                        point_b_lon: 21.002,
                        direction: 1,
                        _padding1: 0,
                        _padding2: 0,
                        tag_set_index: 0,
                    },
                ],
                line_refs: vec![0],
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: vec![RuleRecord {
                    from_lines_offset: 0,
                    from_lines_count: 1,
                    _padding1: 0,
                    to_lines_offset: 1,
                    to_lines_count: 1,
                    rule_type: 1,
                    _padding2: 0,
                    _padding3: 0,
                }],
                rule_line_refs: vec![0, 1],
            },
        );

        write_overlap_manifest(
            &dir,
            OverlapManifestTile {
                id: tile_a,
                bounds: bounds_a,
                path: &tile_a_path,
                point_count: 1,
                line_count: 0,
            },
            OverlapManifestTile {
                id: tile_b,
                bounds: bounds_b,
                path: &tile_b_path,
                point_count: 3,
                line_count: 2,
            },
        );

        let graph = MapDataGraph::new(crate::rmdf::TileManager::new(dir.clone()).unwrap());
        let point = graph.get_point_from_tiles(tile_a, boundary_osm_id);

        assert_eq!(point.rules.len(), 1);
        assert_eq!(
            point.rules[0].from_lines,
            vec![MapDataLineRef::new(tile_b, 0)]
        );
        assert_eq!(
            point.rules[0].to_lines,
            vec![MapDataLineRef::new(tile_b, 1)]
        );
        assert_eq!(point.rules[0].rule_type, MapDataRuleType::NotAllowed);

        fs::remove_dir_all(dir).unwrap();
    }
}
