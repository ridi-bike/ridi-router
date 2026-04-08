const MAX_LOADED_TILES: usize = 100; // Conservative FD limit
const GRID_CELL_PRECISION: u32 = 100;
const GRID_SEARCH_RING_RADIUS: i16 = 20;

use crate::{
    map_data::{
        graph::MapDataLineRef,
        rule::{MapDataRule, MapDataRuleType},
    },
    router::rules::{RouterRules, RulesTagValueAction},
};
use anyhow::{Context, Result};
use geo::{Distance, Haversine, Point};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::format::*;
use super::io::MappedTile;
use ridi_router_common::manifest::TileManifest;

struct LoadedTile {
    tile: MappedTile,
    point_index: HashMap<u64, usize>,
    last_used: AtomicU64,
}

impl LoadedTile {
    fn new(tile: MappedTile, last_used: u64) -> Result<Self> {
        let point_index = tile
            .get_points()?
            .iter()
            .enumerate()
            .map(|(idx, point)| (point.osm_id, idx))
            .collect();

        Ok(Self {
            tile,
            point_index,
            last_used: AtomicU64::new(last_used),
        })
    }

    fn touch(&self, access_epoch: u64) {
        self.last_used.store(access_epoch, Ordering::Relaxed);
    }

    fn last_used(&self) -> u64 {
        self.last_used.load(Ordering::Relaxed)
    }

    fn point(&self, osm_id: u64) -> Result<Option<PointRecord>> {
        let Some(&point_idx) = self.point_index.get(&osm_id) else {
            return Ok(None);
        };
        Ok(self.tile.get_points()?.get(point_idx).copied())
    }
}

pub struct TileManager {
    tile_dir: PathBuf,
    loaded_tiles: HashMap<TileId, LoadedTile>,
    access_epoch: AtomicU64,
    #[cfg(test)]
    loaded_tiles_limit: usize,
    tile_size_degrees: f32,
}

#[derive(Debug, Default)]
struct AvoidTagSet {
    highways: HashSet<String>,
    surfaces: HashSet<String>,
    smoothness: HashSet<String>,
}

#[derive(Debug, Default)]
struct AdjacentLineTags {
    highway: Option<String>,
    surface: Option<String>,
    smoothness: Option<String>,
}

type AdjacentLineData = (usize, u64, f32, f32, u64, f32, f32);
type AdjacentByIdEntry = (TileId, usize, TileId, u64);
const ADJACENT_INLINE_CAPACITY: usize = 8;

#[hotpath::measure_all]
impl TileManager {
    /// Initialize TileManager from directory containing manifest.json
    pub fn new(tile_dir: PathBuf) -> Result<Self> {
        let manifest_path = tile_dir.join("manifest.json");
        let manifest_file =
            std::fs::File::open(&manifest_path).context("Failed to open manifest.json")?;
        let manifest: TileManifest =
            serde_json::from_reader(manifest_file).context("Failed to parse manifest.json")?;

        Ok(Self::from_manifest(manifest, tile_dir))
    }

    /// Create TileManager from manifest directly
    pub fn from_manifest(manifest: TileManifest, tile_dir: PathBuf) -> Self {
        Self {
            tile_dir,
            loaded_tiles: HashMap::new(),
            access_epoch: AtomicU64::new(0),
            #[cfg(test)]
            loaded_tiles_limit: MAX_LOADED_TILES,
            tile_size_degrees: manifest.tile_size_degrees,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_manifest_with_loaded_tiles_limit(
        manifest: TileManifest,
        tile_dir: PathBuf,
        loaded_tiles_limit: usize,
    ) -> Self {
        assert!(
            loaded_tiles_limit > 0,
            "test loaded tiles limit must be positive"
        );
        assert!(
            loaded_tiles_limit <= MAX_LOADED_TILES,
            "test loaded tiles limit {} exceeds capacity {}",
            loaded_tiles_limit,
            MAX_LOADED_TILES
        );

        let mut manager = Self::from_manifest(manifest, tile_dir);
        manager.loaded_tiles_limit = loaded_tiles_limit;
        manager
    }

    #[cfg(test)]
    fn loaded_tiles_limit(&self) -> usize {
        self.loaded_tiles_limit
    }

    #[cfg(not(test))]
    fn loaded_tiles_limit(&self) -> usize {
        MAX_LOADED_TILES
    }

    /// Compute TileId for given coordinates
    pub fn tile_id_for_coords(&self, lat: f32, lon: f32) -> TileId {
        TileId::from_coords(lat, lon, self.tile_size_degrees)
    }

    fn next_access_epoch(&self) -> u64 {
        self.access_epoch.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn touch_loaded_tile(&self, loaded_tile: &LoadedTile) {
        loaded_tile.touch(self.next_access_epoch());
    }

    fn loaded_tile(&self, tile_id: TileId) -> Option<&LoadedTile> {
        let loaded_tile = self.loaded_tiles.get(&tile_id)?;
        self.touch_loaded_tile(loaded_tile);
        Some(loaded_tile)
    }

    fn unload_if_needed(&mut self) -> Result<()> {
        if self.loaded_tiles.len() <= self.loaded_tiles_limit() {
            return Ok(());
        }

        let Some(tile_id) = self
            .loaded_tiles
            .iter()
            .min_by_key(|(_, loaded_tile)| loaded_tile.last_used())
            .map(|(tile_id, _)| *tile_id)
        else {
            return Ok(());
        };

        self.loaded_tiles.remove(&tile_id);
        tracing::debug!(?tile_id, "Unloaded tile");
        Ok(())
    }

    /// Ensure tile is loaded (load if not already)
    fn ensure_tile_loaded(&mut self, tile_id: TileId) -> Result<()> {
        if let Some(loaded_tile) = self.loaded_tiles.get(&tile_id) {
            self.touch_loaded_tile(loaded_tile);
            return Ok(());
        }

        let filename = tile_id.to_filename();
        let filepath = self.tile_dir.join(&filename);

        let mapped_tile = MappedTile::load(&filepath)
            .with_context(|| format!("Failed to load tile: {:?}", filename))?;
        let loaded_tile = LoadedTile::new(mapped_tile, self.next_access_epoch())?;

        self.loaded_tiles.insert(tile_id, loaded_tile);
        self.unload_if_needed()?;

        Ok(())
    }

    /// Get point by OSM ID within a specific tile
    pub fn get_point_by_id(&mut self, tile_id: TileId, osm_id: u64) -> Result<PointRecord> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_point_by_id_if_loaded(tile_id, osm_id)?
            .with_context(|| format!("Point {} not found in tile {:?}", osm_id, tile_id))
    }

    pub(crate) fn get_point_by_id_if_loaded(
        &self,
        tile_id: TileId,
        osm_id: u64,
    ) -> Result<Option<PointRecord>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };

        loaded_tile.point(osm_id)
    }

    /// Get line by index within a specific tile
    pub fn get_line_by_index(&mut self, tile_id: TileId, line_index: usize) -> Result<LineRecord> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_line_by_index_if_loaded(tile_id, line_index)?
            .with_context(|| {
                format!(
                    "Line index {} out of bounds in tile {:?}",
                    line_index, tile_id
                )
            })
    }

    pub(crate) fn get_line_by_index_if_loaded(
        &self,
        tile_id: TileId,
        line_index: usize,
    ) -> Result<Option<LineRecord>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let lines = loaded_tile.tile.get_lines()?;

        Ok(lines.get(line_index).copied())
    }

    pub(crate) fn get_rules_for_point(
        &mut self,
        tile_id: TileId,
        point: &PointRecord,
    ) -> Result<Vec<MapDataRule>> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_rules_for_point_if_loaded(tile_id, point)?
            .with_context(|| format!("Tile {:?} not loaded after point rule lookup", tile_id))
    }

    pub(crate) fn get_rules_for_point_if_loaded(
        &self,
        tile_id: TileId,
        point: &PointRecord,
    ) -> Result<Option<Vec<MapDataRule>>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let tile = &loaded_tile.tile;
        let rule_records = tile.get_rules()?;
        let payload = tile.get_rule_line_refs_payload()?;
        let (rules_start, rules_end) =
            Self::checked_range(point.rules_offset, point.rules_count, rule_records.len())
                .with_context(|| {
                    format!(
                        "Rule slice for point {} is out of bounds in tile {:?}",
                        point.osm_id, tile_id
                    )
                })?;

        let selected_rules = &rule_records[rules_start..rules_end];
        let mut rules = Vec::with_capacity(selected_rules.len());

        for rule_record in selected_rules {
            let from_line_indices = Self::slice_rule_line_refs(
                payload,
                rule_record.from_lines_offset,
                rule_record.from_lines_count,
            )
            .with_context(|| {
                format!(
                    "from-lines slice for point {} rule is out of bounds in tile {:?}",
                    point.osm_id, tile_id
                )
            })?;
            let to_line_indices = Self::slice_rule_line_refs(
                payload,
                rule_record.to_lines_offset,
                rule_record.to_lines_count,
            )
            .with_context(|| {
                format!(
                    "to-lines slice for point {} rule is out of bounds in tile {:?}",
                    point.osm_id, tile_id
                )
            })?;

            rules.push(MapDataRule {
                from_lines: Self::map_rule_line_refs(tile_id, from_line_indices),
                to_lines: Self::map_rule_line_refs(tile_id, to_line_indices),
                rule_type: Self::deserialize_rule_type(rule_record.rule_type),
            });
        }

        Ok(Some(rules))
    }

    pub(crate) fn get_line_indices_for_point(
        &mut self,
        tile_id: TileId,
        point: &PointRecord,
    ) -> Result<&[u64]> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_line_indices_for_point_if_loaded(tile_id, point)?
            .with_context(|| format!("Tile {:?} not loaded after point line lookup", tile_id))
    }

    pub(crate) fn get_line_indices_for_point_if_loaded(
        &self,
        tile_id: TileId,
        point: &PointRecord,
    ) -> Result<Option<&[u64]>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let line_refs = loaded_tile.tile.get_line_refs()?;
        let (start, end) =
            Self::checked_range(point.lines_offset, point.lines_count, line_refs.len())
                .with_context(|| {
                    format!(
                        "Line slice for point {} is out of bounds in tile {:?}",
                        point.osm_id, tile_id
                    )
                })?;

        Ok(Some(&line_refs[start..end]))
    }

    /// Get closest point to coordinates with filtering
    /// Returns (TileId, osm_id) tuple
    pub fn get_closest_to_coords(
        &mut self,
        lat: f32,
        lon: f32,
        rules: &RouterRules,
        avoid_proximity_to_residential: bool,
        limit_to_hw_tags: Option<&[&'static str]>,
    ) -> Result<Option<(TileId, u64)>> {
        let tile_id = TileId::from_coords(lat, lon, self.tile_size_degrees);
        self.ensure_tile_loaded(tile_id)?;

        let loaded_tile = self.loaded_tiles.get(&tile_id).unwrap();
        let tile = &loaded_tile.tile;
        let points = tile.get_points()?;
        let line_refs = tile.get_line_refs()?;
        let avoid_tags = Self::collect_avoid_tags(rules);
        let check_limit_tags = Self::should_check_limit_tags(limit_to_hw_tags, &avoid_tags);

        let closest = if let Some(closest) = Self::find_closest_in_grid_rings(
            tile_id,
            tile,
            points,
            line_refs,
            lat,
            lon,
            avoid_proximity_to_residential,
            &avoid_tags,
            limit_to_hw_tags,
            check_limit_tags,
        )? {
            Some(closest)
        } else {
            Self::find_closest_in_points(
                tile_id,
                tile,
                points,
                line_refs,
                lat,
                lon,
                avoid_proximity_to_residential,
                &avoid_tags,
                limit_to_hw_tags,
                check_limit_tags,
            )?
        };

        Ok(closest.map(|(id_tuple, _)| id_tuple))
    }

    fn find_closest_in_grid_rings(
        tile_id: TileId,
        tile: &MappedTile,
        points: &[PointRecord],
        line_refs: &[u64],
        lat: f32,
        lon: f32,
        avoid_proximity_to_residential: bool,
        avoid_tags: &AvoidTagSet,
        limit_to_hw_tags: Option<&[&'static str]>,
        check_limit_tags: bool,
    ) -> Result<Option<((TileId, u64), f32)>> {
        let spatial_index = tile.get_spatial_index()?;
        let (query_lat_cell, query_lon_cell) =
            Self::query_cell_coords(lat, lon, GRID_CELL_PRECISION);
        let mut closest = None;

        for ring in 0..=GRID_SEARCH_RING_RADIUS {
            for cell_id in Self::grid_ring_cell_ids(query_lat_cell, query_lon_cell, ring) {
                let Some(grid_cell) = Self::find_grid_cell_entry(spatial_index, cell_id) else {
                    continue;
                };
                let cell_points =
                    Self::iter_points_for_grid_cell(points, grid_cell).with_context(|| {
                        format!(
                            "Spatial index point slice for cell {} is out of bounds in tile {:?}",
                            grid_cell.cell_id, tile_id
                        )
                    })?;

                Self::update_closest_for_points(
                    tile_id,
                    tile,
                    line_refs,
                    cell_points,
                    lat,
                    lon,
                    avoid_proximity_to_residential,
                    avoid_tags,
                    limit_to_hw_tags,
                    check_limit_tags,
                    &mut closest,
                )?;
            }
        }

        Ok(closest)
    }

    fn find_closest_in_points(
        tile_id: TileId,
        tile: &MappedTile,
        points: &[PointRecord],
        line_refs: &[u64],
        lat: f32,
        lon: f32,
        avoid_proximity_to_residential: bool,
        avoid_tags: &AvoidTagSet,
        limit_to_hw_tags: Option<&[&'static str]>,
        check_limit_tags: bool,
    ) -> Result<Option<((TileId, u64), f32)>> {
        let mut closest = None;
        Self::update_closest_for_points(
            tile_id,
            tile,
            line_refs,
            points,
            lat,
            lon,
            avoid_proximity_to_residential,
            avoid_tags,
            limit_to_hw_tags,
            check_limit_tags,
            &mut closest,
        )?;
        Ok(closest)
    }

    fn update_closest_for_points(
        tile_id: TileId,
        tile: &MappedTile,
        line_refs: &[u64],
        points: &[PointRecord],
        lat: f32,
        lon: f32,
        avoid_proximity_to_residential: bool,
        avoid_tags: &AvoidTagSet,
        limit_to_hw_tags: Option<&[&'static str]>,
        check_limit_tags: bool,
        closest: &mut Option<((TileId, u64), f32)>,
    ) -> Result<()> {
        for point in points {
            if !Self::point_matches_filters(
                tile,
                line_refs,
                point,
                avoid_proximity_to_residential,
                avoid_tags,
                limit_to_hw_tags,
                check_limit_tags,
            )? {
                continue;
            }

            let dist = Self::haversine_distance_m(lat, lon, point.lat, point.lon);
            let should_replace = closest
                .as_ref()
                .is_none_or(|(_, min_dist)| dist < *min_dist);

            if should_replace {
                *closest = Some(((tile_id, point.osm_id), dist));
            }
        }

        Ok(())
    }

    fn query_cell_coords(lat: f32, lon: f32, precision: u32) -> (i16, i16) {
        let lat_rounded = (lat * precision as f32).round() as i16;
        let lon_rounded = (lon * precision as f32).round() as i16;
        (lat_rounded, lon_rounded)
    }

    fn encode_grid_cell_id(lat_rounded: i16, lon_rounded: i16) -> u32 {
        ((lat_rounded as u32) << 16) | (lon_rounded as u32 & 0xFFFF)
    }

    fn grid_ring_cell_ids(query_lat_cell: i16, query_lon_cell: i16, ring: i16) -> Vec<u32> {
        if ring == 0 {
            return vec![Self::encode_grid_cell_id(query_lat_cell, query_lon_cell)];
        }

        let mut cell_ids = Vec::with_capacity((ring as usize) * 8);
        let lat_min = query_lat_cell - ring;
        let lat_max = query_lat_cell + ring;
        let lon_min = query_lon_cell - ring;
        let lon_max = query_lon_cell + ring;

        for lat_cell in lat_min..=lat_max {
            for lon_cell in lon_min..=lon_max {
                if (lat_cell - query_lat_cell).abs() == ring
                    || (lon_cell - query_lon_cell).abs() == ring
                {
                    cell_ids.push(Self::encode_grid_cell_id(lat_cell, lon_cell));
                }
            }
        }

        cell_ids
    }

    fn find_grid_cell_entry(
        spatial_index: &[GridCellEntry],
        cell_id: u32,
    ) -> Option<&GridCellEntry> {
        spatial_index
            .binary_search_by_key(&cell_id, |entry| entry.cell_id)
            .ok()
            .map(|index| &spatial_index[index])
    }

    fn iter_points_for_grid_cell<'a>(
        points: &'a [PointRecord],
        grid_cell: &GridCellEntry,
    ) -> Result<&'a [PointRecord]> {
        let (start, end) = Self::checked_range(
            grid_cell.points_offset,
            grid_cell.points_count,
            points.len(),
        )?;
        Ok(&points[start..end])
    }

    fn collect_avoid_tags(rules: &RouterRules) -> AvoidTagSet {
        fn collect_tag_values(
            tag_rules: &Option<HashMap<String, RulesTagValueAction>>,
        ) -> HashSet<String> {
            tag_rules
                .iter()
                .flat_map(|tag_rules| tag_rules.iter())
                .filter_map(|(value, action)| match action {
                    RulesTagValueAction::Avoid => Some(value.clone()),
                    RulesTagValueAction::Priority { .. } => None,
                })
                .collect()
        }

        AvoidTagSet {
            highways: collect_tag_values(&rules.highway),
            surfaces: collect_tag_values(&rules.surface),
            smoothness: collect_tag_values(&rules.smoothness),
        }
    }

    fn should_check_limit_tags(
        limit_to_hw_tags: Option<&[&'static str]>,
        avoid_tags: &AvoidTagSet,
    ) -> bool {
        let Some(limit_to_hw_tags) = limit_to_hw_tags else {
            return false;
        };

        limit_to_hw_tags
            .iter()
            .any(|highway| !avoid_tags.highways.contains(*highway))
    }

    fn adjacent_line_tags(
        tile: &MappedTile,
        line_refs: &[u64],
        point: &PointRecord,
    ) -> Result<Vec<AdjacentLineTags>> {
        let lines = tile.get_lines()?;
        let (start, end) =
            Self::checked_range(point.lines_offset, point.lines_count, line_refs.len())
                .with_context(|| {
                    format!("Line slice for point {} is out of bounds", point.osm_id)
                })?;

        let mut adjacent_tags = Vec::with_capacity(end - start);

        for &line_index in &line_refs[start..end] {
            let line_index = usize::try_from(line_index)
                .with_context(|| format!("Line index {} does not fit usize", line_index))?;
            let line = *lines.get(line_index).with_context(|| {
                format!(
                    "Line index {} for point {} is out of bounds",
                    line_index, point.osm_id
                )
            })?;
            let tag_set = tile.get_tag_set(line.tag_set_index).with_context(|| {
                format!(
                    "Tag set index {} for point {} line {} is invalid",
                    line.tag_set_index, point.osm_id, line_index
                )
            })?;

            adjacent_tags.push(AdjacentLineTags {
                highway: Self::optional_tag_value(tile, tag_set.highway_idx)?,
                surface: Self::optional_tag_value(tile, tag_set.surface_idx)?,
                smoothness: Self::optional_tag_value(tile, tag_set.smoothness_idx)?,
            });
        }

        Ok(adjacent_tags)
    }

    fn optional_tag_value(tile: &MappedTile, tag_value_idx: u32) -> Result<Option<String>> {
        if tag_value_idx == TagSetRecord::NONE {
            return Ok(None);
        }

        Ok(Some(tile.get_tag_value(tag_value_idx)?.to_string()))
    }

    fn point_matches_filters(
        tile: &MappedTile,
        line_refs: &[u64],
        point: &PointRecord,
        avoid_proximity_to_residential: bool,
        avoid_tags: &AvoidTagSet,
        limit_to_hw_tags: Option<&[&'static str]>,
        check_limit_tags: bool,
    ) -> Result<bool> {
        if point.lines_count == 0 {
            return Ok(false);
        }

        if avoid_proximity_to_residential && point.residential_in_proximity() {
            return Ok(false);
        }

        let adjacent_tags = Self::adjacent_line_tags(tile, line_refs, point)?;

        let matches_avoid_rule = adjacent_tags.iter().any(|tags| {
            tags.highway
                .as_ref()
                .is_some_and(|value| avoid_tags.highways.contains(value))
                || tags
                    .surface
                    .as_ref()
                    .is_some_and(|value| avoid_tags.surfaces.contains(value))
                || tags
                    .smoothness
                    .as_ref()
                    .is_some_and(|value| avoid_tags.smoothness.contains(value))
        });
        if matches_avoid_rule {
            return Ok(false);
        }

        if !check_limit_tags {
            return Ok(true);
        }

        let Some(limit_to_hw_tags) = limit_to_hw_tags else {
            return Ok(true);
        };

        Ok(adjacent_tags.iter().any(|tags| {
            tags.highway
                .as_deref()
                .is_some_and(|value| limit_to_hw_tags.iter().any(|allowed| *allowed == value))
        }))
    }

    fn haversine_distance_m(from_lat: f32, from_lon: f32, to_lat: f32, to_lon: f32) -> f32 {
        Haversine.distance(Point::new(from_lon, from_lat), Point::new(to_lon, to_lat))
    }

    /// Get adjacent lines and points from a point using only already loaded tiles.
    /// Returns Ok(None) if the center tile is not loaded or if a cross-tile neighbor would
    /// require loading another tile.
    /// Returns Vec<(line_tile_id, line_index, other_point_tile_id, other_point_osm_id)>
    pub fn get_adjacent_by_id_if_loaded(
        &self,
        tile_id: TileId,
        osm_id: u64,
    ) -> Result<Option<Vec<AdjacentByIdEntry>>> {
        let Some(line_indices_and_data) =
            self.collect_adjacent_line_data_if_loaded(tile_id, osm_id)?
        else {
            return Ok(None);
        };

        let mut result = SmallVec::<[AdjacentByIdEntry; ADJACENT_INLINE_CAPACITY]>::with_capacity(
            line_indices_and_data.len(),
        );

        for (
            line_idx,
            point_a_osm_id,
            point_a_lat,
            point_a_lon,
            point_b_osm_id,
            point_b_lat,
            point_b_lon,
        ) in line_indices_and_data
        {
            let (other_osm_id, other_lat, other_lon) = if point_a_osm_id == osm_id {
                (point_b_osm_id, point_b_lat, point_b_lon)
            } else {
                (point_a_osm_id, point_a_lat, point_a_lon)
            };

            let other_tile_id = TileId::from_coords(other_lat, other_lon, self.tile_size_degrees);

            if other_tile_id != tile_id && self.loaded_tile(other_tile_id).is_none() {
                return Ok(None);
            }

            result.push((tile_id, line_idx, other_tile_id, other_osm_id));
        }

        Ok(Some(result.into_vec()))
    }

    /// Get adjacent lines and points from a point (handles border crossing)
    /// Returns Vec<(line_tile_id, line_index, other_point_tile_id, other_point_osm_id)>
    pub fn get_adjacent_by_id(
        &mut self,
        tile_id: TileId,
        osm_id: u64,
    ) -> Result<Vec<AdjacentByIdEntry>> {
        self.ensure_tile_loaded(tile_id)?;

        let line_indices_and_data = self
            .collect_adjacent_line_data_if_loaded(tile_id, osm_id)?
            .with_context(|| format!("Tile {:?} not loaded after adjacency lookup", tile_id))?;

        let mut result = SmallVec::<[AdjacentByIdEntry; ADJACENT_INLINE_CAPACITY]>::with_capacity(
            line_indices_and_data.len(),
        );

        for (
            line_idx,
            point_a_osm_id,
            point_a_lat,
            point_a_lon,
            point_b_osm_id,
            point_b_lat,
            point_b_lon,
        ) in line_indices_and_data
        {
            // Determine which endpoint is the "other" point
            let (other_osm_id, other_lat, other_lon) = if point_a_osm_id == osm_id {
                (point_b_osm_id, point_b_lat, point_b_lon)
            } else {
                (point_a_osm_id, point_a_lat, point_a_lon)
            };

            // Determine which tile contains the other point
            let other_tile_id = TileId::from_coords(other_lat, other_lon, self.tile_size_degrees);

            // Check if we need to load a different tile
            if other_tile_id != tile_id {
                // Border crossing detected
                match self.ensure_tile_loaded(other_tile_id) {
                    Ok(_) => {
                        // Tile loaded successfully
                    }
                    Err(_) => {
                        // Tile missing - skip this line (dead-end)
                        tracing::warn!(
                            "Tile {:?} not available, treating line as dead-end",
                            other_tile_id
                        );
                        continue;
                    }
                }
            }

            result.push((tile_id, line_idx, other_tile_id, other_osm_id));
        }

        Ok(result.into_vec())
    }

    fn collect_adjacent_line_data_if_loaded(
        &self,
        tile_id: TileId,
        osm_id: u64,
    ) -> Result<Option<SmallVec<[AdjacentLineData; ADJACENT_INLINE_CAPACITY]>>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let tile = &loaded_tile.tile;
        let Some(point) = loaded_tile.point(osm_id)? else {
            return Ok(None);
        };

        let lines = tile.get_lines()?;
        let line_refs_array = tile.get_line_refs()?;

        let (start, end) =
            Self::checked_range(point.lines_offset, point.lines_count, line_refs_array.len())
                .context("Point line slice out of bounds in tile")?;

        let mut adjacent_line_data =
            SmallVec::<[AdjacentLineData; ADJACENT_INLINE_CAPACITY]>::with_capacity(end - start);

        for &line_idx in &line_refs_array[start..end] {
            let line = &lines[line_idx as usize];
            adjacent_line_data.push((
                line_idx as usize,
                line.point_a_osm_id,
                line.point_a_lat,
                line.point_a_lon,
                line.point_b_osm_id,
                line.point_b_lat,
                line.point_b_lon,
            ));
        }

        Ok(Some(adjacent_line_data))
    }

    fn checked_range(offset: u64, count: u32, len: usize) -> Result<(usize, usize)> {
        let start = usize::try_from(offset).context("offset must fit in usize")?;
        let count = usize::try_from(count).context("count must fit in usize")?;
        let end = start.checked_add(count).context("slice end overflow")?;

        if end > len {
            anyhow::bail!("slice {}..{} exceeds length {}", start, end, len);
        }

        Ok((start, end))
    }

    fn slice_rule_line_refs<'a>(payload: &'a [u64], offset: u64, count: u32) -> Result<&'a [u64]> {
        let (start, end) = Self::checked_range(offset, count, payload.len())?;
        Ok(&payload[start..end])
    }

    fn map_rule_line_refs(tile_id: TileId, line_indices: &[u64]) -> Vec<MapDataLineRef> {
        let mut line_refs = Vec::with_capacity(line_indices.len());
        for &line_index in line_indices {
            line_refs.push(MapDataLineRef::new(tile_id, line_index));
        }
        line_refs
    }

    fn deserialize_rule_type(rule_type: u8) -> MapDataRuleType {
        match rule_type {
            0 => MapDataRuleType::OnlyAllowed,
            1 => MapDataRuleType::NotAllowed,
            _ => panic!("Unknown serialized rule type {rule_type}"),
        }
    }

    /// Get tag value string from tile
    pub fn get_tag_value(&mut self, tile_id: TileId, tag_value_idx: u32) -> Result<String> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_tag_value_if_loaded(tile_id, tag_value_idx)?
            .with_context(|| format!("Tile {:?} not loaded after tag value lookup", tile_id))
    }

    pub fn get_tag_value_if_loaded(
        &self,
        tile_id: TileId,
        tag_value_idx: u32,
    ) -> Result<Option<String>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let value_str = loaded_tile.tile.get_tag_value(tag_value_idx)?;

        Ok(Some(value_str.to_string()))
    }

    /// Get tag set record from tile
    pub fn get_tag_set_record(
        &mut self,
        tile_id: TileId,
        tag_set_idx: u32,
    ) -> Result<TagSetRecord> {
        self.ensure_tile_loaded(tile_id)?;
        self.get_tag_set_record_if_loaded(tile_id, tag_set_idx)?
            .with_context(|| format!("Tile {:?} not loaded after tag set lookup", tile_id))
    }

    pub fn get_tag_set_record_if_loaded(
        &self,
        tile_id: TileId,
        tag_set_idx: u32,
    ) -> Result<Option<TagSetRecord>> {
        let Some(loaded_tile) = self.loaded_tile(tile_id) else {
            return Ok(None);
        };
        let tag_set = loaded_tile.tile.get_tag_set(tag_set_idx)?;

        Ok(Some(tag_set))
    }

    #[cfg(test)]
    fn spatial_index_entry(
        lat: f32,
        lon: f32,
        points_offset: u64,
        points_count: u32,
    ) -> GridCellEntry {
        GridCellEntry {
            cell_id: GridCellEntry::encode_cell_id(lat, lon, GRID_CELL_PRECISION),
            _padding1: 0,
            points_offset,
            points_count,
            _padding2: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn loaded_tile_count(&self) -> usize {
        self.loaded_tiles.len()
    }

    #[cfg(test)]
    pub(crate) fn is_tile_loaded(&self, tile_id: TileId) -> bool {
        self.loaded_tiles.contains_key(&tile_id)
    }

    #[cfg(test)]
    pub(crate) fn loaded_tile_order_for_test(&self) -> Vec<TileId> {
        let mut entries = self
            .loaded_tiles
            .iter()
            .map(|(tile_id, loaded_tile)| (*tile_id, loaded_tile.last_used()))
            .collect::<Vec<_>>();
        entries.sort_by(
            |(left_tile_id, left_last_used), (right_tile_id, right_last_used)| {
                left_last_used
                    .cmp(right_last_used)
                    .then_with(|| left_tile_id.row.cmp(&right_tile_id.row))
                    .then_with(|| left_tile_id.col.cmp(&right_tile_id.col))
            },
        );
        entries.into_iter().map(|(tile_id, _)| tile_id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_data::rule::MapDataRuleType;
    use crate::router::rules::{RouterRules, RulesTagValueAction};
    use ridi_router_common::format::RuleRecord;
    use ridi_router_test_support::rmdf::{
        create_missing_neighbor_fixture, empty_neighbors, manifest_bounds, unique_test_dir,
        write_manifest, write_tile, LineRecord, PointRecord, TagSetRecord, TileBounds, TileId,
        TileManifest, TileMetadata, TileNeighbors, TileSpec, SYNTHETIC_TILE_BOUNDS,
        SYNTHETIC_TILE_ID, SYNTHETIC_TILE_SIZE_DEGREES,
    };
    use std::{collections::HashMap, fs, path::PathBuf};

    fn test_manifest() -> TileManifest {
        TileManifest {
            version: "test".to_string(),
            tile_size_degrees: 1.0,
            format_version: 1,
            generated_at: "2026-04-05T00:00:00Z".to_string(),
            source_files: Vec::new(),
            tiles: Vec::new(),
        }
    }

    #[derive(Debug)]
    struct SyntheticLruFixture {
        dir: PathBuf,
        manifest: TileManifest,
        tiles: Vec<TileId>,
        point_ids: Vec<u64>,
    }

    #[derive(Debug)]
    struct SyntheticCrossTileFixture {
        dir: PathBuf,
        tile_a: TileId,
        tile_b: TileId,
        center_osm_id: u64,
        in_tile_neighbor_osm_id: u64,
        cross_tile_neighbor_osm_id: u64,
    }

    #[derive(Debug)]
    struct SyntheticClosestPointFixture {
        dir: PathBuf,
        tile_id: TileId,
    }

    fn bounds_for_tile(tile_id: TileId) -> TileBounds {
        let lon_min = tile_id.col as f32 - 180.0;
        let lat_min = tile_id.row as f32 - 90.0;
        TileBounds {
            lat_min,
            lat_max: lat_min + 1.0,
            lon_min,
            lon_max: lon_min + 1.0,
        }
    }

    fn point_record_with_flags(
        osm_id: u64,
        lat: f32,
        lon: f32,
        lines_offset: u64,
        lines_count: u32,
        flags: u16,
    ) -> PointRecord {
        PointRecord {
            osm_id,
            lat,
            lon,
            lines_offset,
            lines_count,
            _padding1: 0,
            rules_offset: 0,
            rules_count: 0,
            flags,
            _padding2: 0,
        }
    }

    fn point_record(
        osm_id: u64,
        lat: f32,
        lon: f32,
        lines_offset: u64,
        lines_count: u32,
    ) -> PointRecord {
        point_record_with_flags(osm_id, lat, lon, lines_offset, lines_count, 0)
    }

    fn line_record_with_tag_set(
        point_a_osm_id: u64,
        point_a_lat: f32,
        point_a_lon: f32,
        point_b_osm_id: u64,
        point_b_lat: f32,
        point_b_lon: f32,
        tag_set_index: u32,
    ) -> LineRecord {
        LineRecord {
            point_a_osm_id,
            point_a_lat,
            point_a_lon,
            point_b_osm_id,
            point_b_lat,
            point_b_lon,
            direction: 0,
            _padding1: 0,
            _padding2: 0,
            tag_set_index,
        }
    }

    fn line_record(
        point_a_osm_id: u64,
        point_a_lat: f32,
        point_a_lon: f32,
        point_b_osm_id: u64,
        point_b_lat: f32,
        point_b_lon: f32,
    ) -> LineRecord {
        line_record_with_tag_set(
            point_a_osm_id,
            point_a_lat,
            point_a_lon,
            point_b_osm_id,
            point_b_lat,
            point_b_lon,
            0,
        )
    }

    fn tag_set_record(
        highway_idx: Option<u32>,
        surface_idx: Option<u32>,
        smoothness_idx: Option<u32>,
    ) -> TagSetRecord {
        TagSetRecord {
            name_idx: TagSetRecord::NONE,
            hw_ref_idx: TagSetRecord::NONE,
            highway_idx: highway_idx.unwrap_or(TagSetRecord::NONE),
            surface_idx: surface_idx.unwrap_or(TagSetRecord::NONE),
            smoothness_idx: smoothness_idx.unwrap_or(TagSetRecord::NONE),
        }
    }

    fn create_rule_tile_manager_fixture(
        prefix: &str,
        point: PointRecord,
        rules: Vec<RuleRecord>,
        rule_line_refs: Vec<u64>,
    ) -> (PathBuf, TileId, TileManager) {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![point],
                lines: Vec::new(),
                line_refs: Vec::new(),
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
                    filename: SYNTHETIC_TILE_ID.to_filename(),
                    col: SYNTHETIC_TILE_ID.col,
                    row: SYNTHETIC_TILE_ID.row,
                    bounds: manifest_bounds(SYNTHETIC_TILE_BOUNDS),
                    neighbors: empty_neighbors(),
                    size_bytes: fs::metadata(&tile_path).unwrap().len(),
                    point_count: 1,
                    line_count: 0,
                    checksum: "sha256:test".to_string(),
                    military_geojson_filename: None,
                }],
            },
        );

        (
            dir.clone(),
            SYNTHETIC_TILE_ID,
            TileManager::new(dir).unwrap(),
        )
    }

    fn create_single_point_tile_fixture(prefix: &str, tile_count: usize) -> SyntheticLruFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let row = 100;
        let base_col = 200_u16;
        let mut tiles = Vec::with_capacity(tile_count);
        let mut point_ids = Vec::with_capacity(tile_count);
        let mut manifest_tiles = Vec::with_capacity(tile_count);

        for index in 0..tile_count {
            let tile_id = TileId {
                col: base_col + index as u16,
                row,
            };
            let bounds = bounds_for_tile(tile_id);
            let osm_id = 10_000 + index as u64;
            let tile_path = write_tile(
                &dir,
                &TileSpec {
                    tile_id,
                    bounds,
                    spatial_index: Vec::new(),
                    points: vec![point_record(
                        osm_id,
                        bounds.lat_min + 0.5,
                        bounds.lon_min + 0.5,
                        0,
                        0,
                    )],
                    lines: Vec::new(),
                    line_refs: Vec::new(),
                    tag_values: Vec::new(),
                    tag_sets: Vec::new(),
                    rules: Vec::new(),
                    rule_line_refs: Vec::new(),
                },
            );

            manifest_tiles.push(TileMetadata {
                filename: tile_id.to_filename(),
                col: tile_id.col,
                row: tile_id.row,
                bounds: manifest_bounds(bounds),
                neighbors: empty_neighbors(),
                size_bytes: fs::metadata(&tile_path).unwrap().len(),
                point_count: 1,
                line_count: 0,
                checksum: format!("sha256:lru-{index}"),
                military_geojson_filename: None,
            });
            tiles.push(tile_id);
            point_ids.push(osm_id);
        }

        let manifest = TileManifest {
            version: "test".to_string(),
            tile_size_degrees: 1.0,
            format_version: 1,
            generated_at: "2026-04-05T00:00:00Z".to_string(),
            source_files: vec!["synthetic".to_string()],
            tiles: manifest_tiles,
        };
        write_manifest(&dir, &manifest);

        SyntheticLruFixture {
            dir,
            manifest,
            tiles,
            point_ids,
        }
    }

    fn create_closest_point_fixture(prefix: &str, spec: TileSpec) -> SyntheticClosestPointFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_path = write_tile(&dir, &spec);
        let manifest = TileManifest {
            version: "test".to_string(),
            tile_size_degrees: SYNTHETIC_TILE_SIZE_DEGREES,
            format_version: 1,
            generated_at: "2026-04-05T00:00:00Z".to_string(),
            source_files: vec!["synthetic".to_string()],
            tiles: vec![TileMetadata {
                filename: spec.tile_id.to_filename(),
                col: spec.tile_id.col,
                row: spec.tile_id.row,
                bounds: manifest_bounds(spec.bounds),
                neighbors: empty_neighbors(),
                size_bytes: fs::metadata(&tile_path).unwrap().len(),
                point_count: spec.points.len() as u64,
                line_count: spec.lines.len() as u64,
                checksum: format!("sha256:{prefix}"),
                military_geojson_filename: None,
            }],
        };
        write_manifest(&dir, &manifest);

        SyntheticClosestPointFixture {
            dir,
            tile_id: spec.tile_id,
        }
    }

    fn create_cross_tile_fixture(prefix: &str) -> SyntheticCrossTileFixture {
        let dir = unique_test_dir(prefix);
        fs::create_dir_all(&dir).unwrap();

        let tile_a = TileId { col: 200, row: 100 };
        let tile_b = TileId { col: 201, row: 100 };
        let bounds_a = bounds_for_tile(tile_a);
        let bounds_b = bounds_for_tile(tile_b);

        let center_osm_id = 20_000;
        let in_tile_neighbor_osm_id = 20_001;
        let cross_tile_neighbor_osm_id = 20_002;

        let tile_a_points = vec![
            point_record(center_osm_id, 10.50, 20.95, 0, 2),
            point_record(in_tile_neighbor_osm_id, 10.60, 20.80, 2, 1),
        ];
        let tile_a_lines = vec![
            line_record(
                center_osm_id,
                10.50,
                20.95,
                in_tile_neighbor_osm_id,
                10.60,
                20.80,
            ),
            line_record(
                center_osm_id,
                10.50,
                20.95,
                cross_tile_neighbor_osm_id,
                10.50,
                21.05,
            ),
        ];
        let tile_a_path = write_tile(
            &dir,
            &TileSpec {
                tile_id: tile_a,
                bounds: bounds_a,
                spatial_index: Vec::new(),
                points: tile_a_points,
                lines: tile_a_lines,
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
                points: vec![point_record(cross_tile_neighbor_osm_id, 10.50, 21.05, 0, 0)],
                lines: Vec::new(),
                line_refs: Vec::new(),
                tag_values: Vec::new(),
                tag_sets: Vec::new(),
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let tile_b_filename = tile_b.to_filename();
        let tile_a_filename = tile_a.to_filename();
        let manifest = TileManifest {
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
                    checksum: "sha256:cross-a".to_string(),
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
                    checksum: "sha256:cross-b".to_string(),
                    military_geojson_filename: None,
                },
            ],
        };
        write_manifest(&dir, &manifest);

        SyntheticCrossTileFixture {
            dir,
            tile_a,
            tile_b,
            center_osm_id,
            in_tile_neighbor_osm_id,
            cross_tile_neighbor_osm_id,
        }
    }

    fn touch_tile_via_point_lookup(manager: &mut TileManager, tile_id: TileId, osm_id: u64) {
        let point = manager.get_point_by_id(tile_id, osm_id).unwrap();
        assert_eq!(
            point.osm_id, osm_id,
            "expected point lookup to touch the requested tile"
        );
    }

    #[test]
    fn test_load_manifest() {
        // Assumes Montenegro tiles generated in test setup
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let _manager = TileManager::new(tile_dir).expect("Failed to load TileManager");
    }

    #[test]
    fn test_registry_foundation_initializes_test_hooks() {
        let manager = TileManager::from_manifest_with_loaded_tiles_limit(
            test_manifest(),
            PathBuf::from("test"),
            3,
        );

        assert_eq!(manager.loaded_tile_count(), 0);
        assert!(!manager.is_tile_loaded(TileId { col: 1, row: 2 }));
        assert_eq!(manager.loaded_tile_order_for_test(), Vec::<TileId>::new());
        assert_eq!(manager.loaded_tiles_limit(), 3);
    }

    #[test]
    fn test_lru_overflow_unloads_oldest_tile() {
        let SyntheticLruFixture {
            dir,
            manifest,
            tiles,
            point_ids,
        } = create_single_point_tile_fixture("tile-manager-lru-overflow", 4);
        let mut manager =
            TileManager::from_manifest_with_loaded_tiles_limit(manifest, dir.clone(), 3);

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        touch_tile_via_point_lookup(&mut manager, tiles[1], point_ids[1]);
        touch_tile_via_point_lookup(&mut manager, tiles[2], point_ids[2]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[0], tiles[1], tiles[2]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[3], point_ids[3]);

        assert_eq!(manager.loaded_tile_count(), 3);
        assert!(!manager.is_tile_loaded(tiles[0]));
        assert!(manager.is_tile_loaded(tiles[1]));
        assert!(manager.is_tile_loaded(tiles[2]));
        assert!(manager.is_tile_loaded(tiles[3]));
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[1], tiles[2], tiles[3]]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_lru_hit_refresh_makes_tile_newest() {
        let SyntheticLruFixture {
            dir,
            manifest,
            tiles,
            point_ids,
        } = create_single_point_tile_fixture("tile-manager-lru-hit-refresh", 4);
        let mut manager =
            TileManager::from_manifest_with_loaded_tiles_limit(manifest, dir.clone(), 3);

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        touch_tile_via_point_lookup(&mut manager, tiles[1], point_ids[1]);
        touch_tile_via_point_lookup(&mut manager, tiles[2], point_ids[2]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[0], tiles[1], tiles[2]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[1], tiles[2], tiles[0]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[3], point_ids[3]);

        assert_eq!(manager.loaded_tile_count(), 3);
        assert!(!manager.is_tile_loaded(tiles[1]));
        assert!(manager.is_tile_loaded(tiles[0]));
        assert!(manager.is_tile_loaded(tiles[2]));
        assert!(manager.is_tile_loaded(tiles[3]));
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[2], tiles[0], tiles[3]]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_rules_for_point_hydrates_selected_rules_only() {
        let point = PointRecord {
            osm_id: 77,
            lat: 10.5,
            lon: 20.5,
            lines_offset: 0,
            lines_count: 0,
            _padding1: 0,
            rules_offset: 1,
            rules_count: 2,
            flags: 0,
            _padding2: 0,
        };
        let rules = vec![
            RuleRecord {
                from_lines_offset: 0,
                from_lines_count: 1,
                _padding1: 0,
                to_lines_offset: 1,
                to_lines_count: 1,
                rule_type: 1,
                _padding2: 0,
                _padding3: 0,
            },
            RuleRecord {
                from_lines_offset: 2,
                from_lines_count: 2,
                _padding1: 0,
                to_lines_offset: 4,
                to_lines_count: 1,
                rule_type: 0,
                _padding2: 0,
                _padding3: 0,
            },
            RuleRecord {
                from_lines_offset: 5,
                from_lines_count: 0,
                _padding1: 0,
                to_lines_offset: 5,
                to_lines_count: 0,
                rule_type: 1,
                _padding2: 0,
                _padding3: 0,
            },
        ];
        let (dir, tile_id, mut manager) = create_rule_tile_manager_fixture(
            "tile-manager-rule-hydration",
            point,
            rules,
            vec![9, 10, 20, 21, 30],
        );

        let hydrated = manager
            .get_rules_for_point(tile_id, &point)
            .expect("rules should hydrate");

        assert_eq!(hydrated.len(), 2);
        assert_eq!(hydrated[0].rule_type, MapDataRuleType::OnlyAllowed);
        assert_eq!(
            hydrated[0].from_lines,
            vec![
                MapDataLineRef::new(tile_id, 20),
                MapDataLineRef::new(tile_id, 21)
            ]
        );
        assert_eq!(hydrated[0].to_lines, vec![MapDataLineRef::new(tile_id, 30)]);
        assert_eq!(hydrated[1].rule_type, MapDataRuleType::NotAllowed);
        assert!(hydrated[1].from_lines.is_empty());
        assert!(hydrated[1].to_lines.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_rules_for_point_rejects_out_of_bounds_payload_slice() {
        let point = PointRecord {
            osm_id: 88,
            lat: 10.5,
            lon: 20.5,
            lines_offset: 0,
            lines_count: 0,
            _padding1: 0,
            rules_offset: 0,
            rules_count: 1,
            flags: 0,
            _padding2: 0,
        };
        let rules = vec![RuleRecord {
            from_lines_offset: 0,
            from_lines_count: 2,
            _padding1: 0,
            to_lines_offset: 2,
            to_lines_count: 0,
            rule_type: 0,
            _padding2: 0,
            _padding3: 0,
        }];
        let (dir, tile_id, mut manager) =
            create_rule_tile_manager_fixture("tile-manager-rule-bounds", point, rules, vec![42]);

        let error = manager.get_rules_for_point(tile_id, &point).unwrap_err();
        assert!(error
            .to_string()
            .contains("from-lines slice for point 88 rule is out of bounds"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_lru_hot_tile_survives_colder_unloads() {
        let SyntheticLruFixture {
            dir,
            manifest,
            tiles,
            point_ids,
        } = create_single_point_tile_fixture("tile-manager-lru-hot-tile", 5);
        let mut manager =
            TileManager::from_manifest_with_loaded_tiles_limit(manifest, dir.clone(), 3);

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        touch_tile_via_point_lookup(&mut manager, tiles[1], point_ids[1]);
        touch_tile_via_point_lookup(&mut manager, tiles[2], point_ids[2]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[0], tiles[1], tiles[2]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[1], tiles[2], tiles[0]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[3], point_ids[3]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[2], tiles[0], tiles[3]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[0], point_ids[0]);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[2], tiles[3], tiles[0]]
        );

        touch_tile_via_point_lookup(&mut manager, tiles[4], point_ids[4]);

        assert_eq!(manager.loaded_tile_count(), 3);
        assert!(!manager.is_tile_loaded(tiles[1]));
        assert!(!manager.is_tile_loaded(tiles[2]));
        assert!(manager.is_tile_loaded(tiles[0]));
        assert!(manager.is_tile_loaded(tiles[3]));
        assert!(manager.is_tile_loaded(tiles[4]));
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            vec![tiles[3], tiles[0], tiles[4]]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_lru_exact_max_capacity_transition_unloads_oldest_tile() {
        let SyntheticLruFixture {
            dir,
            manifest,
            tiles,
            point_ids,
        } = create_single_point_tile_fixture(
            "tile-manager-lru-production-boundary",
            MAX_LOADED_TILES + 1,
        );
        let mut manager = TileManager::from_manifest_with_loaded_tiles_limit(
            manifest,
            dir.clone(),
            MAX_LOADED_TILES,
        );

        for index in 0..MAX_LOADED_TILES {
            touch_tile_via_point_lookup(&mut manager, tiles[index], point_ids[index]);
        }

        assert_eq!(manager.loaded_tile_count(), MAX_LOADED_TILES);
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            tiles[..MAX_LOADED_TILES].to_vec()
        );

        touch_tile_via_point_lookup(
            &mut manager,
            tiles[MAX_LOADED_TILES],
            point_ids[MAX_LOADED_TILES],
        );

        assert_eq!(manager.loaded_tile_count(), MAX_LOADED_TILES);
        assert!(!manager.is_tile_loaded(tiles[0]));
        assert!(manager.is_tile_loaded(tiles[1]));
        assert!(manager.is_tile_loaded(tiles[MAX_LOADED_TILES]));
        assert_eq!(
            manager.loaded_tile_order_for_test(),
            tiles[1..=MAX_LOADED_TILES].to_vec()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_highway_allowlist_honors_mixed_adjacent_tags() {
        let candidate_osm_id = 30_000;
        let fixture = create_closest_point_fixture(
            "tile-manager-hw-allowlist-mixed-adjacent",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![point_record(candidate_osm_id, 10.1000, 20.1000, 0, 2)],
                lines: vec![
                    line_record_with_tag_set(
                        candidate_osm_id,
                        10.1000,
                        20.1000,
                        30_001,
                        10.1010,
                        20.1010,
                        0,
                    ),
                    line_record_with_tag_set(
                        candidate_osm_id,
                        10.1000,
                        20.1000,
                        30_002,
                        10.1020,
                        20.1020,
                        1,
                    ),
                ],
                line_refs: vec![0, 1],
                tag_values: vec!["track".to_string(), "secondary".to_string()],
                tag_sets: vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(1), None, None),
                ],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1001,
                &RouterRules::default(),
                false,
                Some(&["secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, candidate_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_highway_allowlist_skips_nearest_disallowed_point() {
        let disallowed_osm_id = 31_000;
        let allowed_osm_id = 31_100;
        let fixture = create_closest_point_fixture(
            "tile-manager-hw-allowlist-nearest-disallowed",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![
                    point_record(disallowed_osm_id, 10.1000, 20.1000, 0, 1),
                    point_record(allowed_osm_id, 10.1020, 20.1000, 1, 1),
                ],
                lines: vec![
                    line_record_with_tag_set(
                        disallowed_osm_id,
                        10.1000,
                        20.1000,
                        31_001,
                        10.1010,
                        20.1010,
                        0,
                    ),
                    line_record_with_tag_set(
                        allowed_osm_id,
                        10.1020,
                        20.1000,
                        31_101,
                        10.1030,
                        20.1010,
                        1,
                    ),
                ],
                line_refs: vec![0, 1],
                tag_values: vec!["track".to_string(), "secondary".to_string()],
                tag_sets: vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(1), None, None),
                ],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1000,
                &RouterRules::default(),
                false,
                Some(&["secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, allowed_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_disables_allowlist_when_all_allowed_highways_are_avoided() {
        let candidate_osm_id = 31_200;
        let rules = RouterRules {
            highway: Some(HashMap::from([
                ("primary".to_string(), RulesTagValueAction::Avoid),
                ("secondary".to_string(), RulesTagValueAction::Avoid),
            ])),
            ..RouterRules::default()
        };
        let fixture = create_closest_point_fixture(
            "tile-manager-hw-allowlist-disabled-by-avoid-rules",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![point_record(candidate_osm_id, 10.1000, 20.1000, 0, 1)],
                lines: vec![line_record_with_tag_set(
                    candidate_osm_id,
                    10.1000,
                    20.1000,
                    31_201,
                    10.1010,
                    20.1010,
                    0,
                )],
                line_refs: vec![0],
                tag_values: vec!["track".to_string()],
                tag_sets: vec![tag_set_record(Some(0), None, None)],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1000,
                &rules,
                false,
                Some(&["primary", "secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, candidate_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_highway_allowlist_rejects_points_without_adjacent_highway_tags() {
        let no_highway_osm_id = 31_300;
        let allowed_osm_id = 31_400;
        let fixture = create_closest_point_fixture(
            "tile-manager-hw-allowlist-rejects-no-highway-tags",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![
                    point_record(no_highway_osm_id, 10.1000, 20.1000, 0, 1),
                    point_record(allowed_osm_id, 10.1030, 20.1000, 1, 1),
                ],
                lines: vec![
                    line_record_with_tag_set(
                        no_highway_osm_id,
                        10.1000,
                        20.1000,
                        31_301,
                        10.1010,
                        20.1010,
                        0,
                    ),
                    line_record_with_tag_set(
                        allowed_osm_id,
                        10.1030,
                        20.1000,
                        31_401,
                        10.1040,
                        20.1010,
                        1,
                    ),
                ],
                line_refs: vec![0, 1],
                tag_values: vec!["gravel".to_string(), "secondary".to_string()],
                tag_sets: vec![
                    tag_set_record(None, Some(0), None),
                    tag_set_record(Some(1), None, None),
                ],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1000,
                &RouterRules::default(),
                false,
                Some(&["secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, allowed_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_find_closest_in_grid_rings_returns_stage_1_match() {
        let candidate_osm_id = 34_000;
        let fixture = create_closest_point_fixture(
            "tile-manager-stage-1-ring-scan-success",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: vec![TileManager::spatial_index_entry(10.1200, 20.1000, 0, 1)],
                points: vec![point_record(candidate_osm_id, 10.1200, 20.1000, 0, 1)],
                lines: vec![line_record_with_tag_set(
                    candidate_osm_id,
                    10.1200,
                    20.1000,
                    34_001,
                    10.1210,
                    20.1010,
                    0,
                )],
                line_refs: vec![0],
                tag_values: vec!["secondary".to_string()],
                tag_sets: vec![tag_set_record(Some(0), None, None)],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        manager.ensure_tile_loaded(fixture.tile_id).unwrap();

        let loaded_tile = manager.loaded_tiles.get(&fixture.tile_id).unwrap();
        let tile = &loaded_tile.tile;
        let points = tile.get_points().unwrap();
        let line_refs = tile.get_line_refs().unwrap();
        let avoid_tags = TileManager::collect_avoid_tags(&RouterRules::default());

        let closest = TileManager::find_closest_in_grid_rings(
            fixture.tile_id,
            tile,
            points,
            line_refs,
            10.1001,
            20.1000,
            false,
            &avoid_tags,
            Some(&["secondary"]),
            TileManager::should_check_limit_tags(Some(&["secondary"]), &avoid_tags),
        )
        .unwrap();

        assert_eq!(
            closest.map(|(id_tuple, _)| id_tuple),
            Some((fixture.tile_id, candidate_osm_id))
        );

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_full_tile_fallback_returns_far_same_tile_candidate() {
        let disallowed_osm_id = 35_000;
        let allowed_osm_id = 35_100;
        let fixture = create_closest_point_fixture(
            "tile-manager-full-tile-fallback",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: vec![
                    TileManager::spatial_index_entry(10.1000, 20.1000, 0, 1),
                    TileManager::spatial_index_entry(10.4000, 20.1000, 1, 1),
                ],
                points: vec![
                    point_record(disallowed_osm_id, 10.1000, 20.1000, 0, 1),
                    point_record(allowed_osm_id, 10.4000, 20.1000, 1, 1),
                ],
                lines: vec![
                    line_record_with_tag_set(
                        disallowed_osm_id,
                        10.1000,
                        20.1000,
                        35_001,
                        10.1010,
                        20.1010,
                        0,
                    ),
                    line_record_with_tag_set(
                        allowed_osm_id,
                        10.4000,
                        20.1000,
                        35_101,
                        10.4010,
                        20.1010,
                        1,
                    ),
                ],
                line_refs: vec![0, 1],
                tag_values: vec!["track".to_string(), "secondary".to_string()],
                tag_sets: vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(1), None, None),
                ],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1000,
                &RouterRules::default(),
                false,
                Some(&["secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, allowed_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_get_closest_to_coords_rules_filtering_rejects_point_when_any_adjacent_line_matches() {
        let cases = vec![
            (
                "highway",
                RouterRules {
                    highway: Some(HashMap::from([(
                        "track".to_string(),
                        RulesTagValueAction::Avoid,
                    )])),
                    ..RouterRules::default()
                },
                vec!["secondary".to_string(), "track".to_string()],
                vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(1), None, None),
                ],
            ),
            (
                "surface",
                RouterRules {
                    surface: Some(HashMap::from([(
                        "gravel".to_string(),
                        RulesTagValueAction::Avoid,
                    )])),
                    ..RouterRules::default()
                },
                vec!["secondary".to_string(), "gravel".to_string()],
                vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(0), Some(1), None),
                ],
            ),
            (
                "smoothness",
                RouterRules {
                    smoothness: Some(HashMap::from([(
                        "bad".to_string(),
                        RulesTagValueAction::Avoid,
                    )])),
                    ..RouterRules::default()
                },
                vec!["secondary".to_string(), "bad".to_string()],
                vec![
                    tag_set_record(Some(0), None, None),
                    tag_set_record(Some(0), None, Some(1)),
                ],
            ),
        ];

        for (label, rules, tag_values, tag_sets) in cases {
            let rejected_osm_id = 32_000;
            let accepted_osm_id = 32_100;
            let fixture = create_closest_point_fixture(
                &format!("tile-manager-rules-filtering-{label}"),
                TileSpec {
                    tile_id: SYNTHETIC_TILE_ID,
                    bounds: SYNTHETIC_TILE_BOUNDS,
                    spatial_index: Vec::new(),
                    points: vec![
                        point_record(rejected_osm_id, 10.1000, 20.1000, 0, 2),
                        point_record(accepted_osm_id, 10.1030, 20.1000, 2, 1),
                    ],
                    lines: vec![
                        line_record_with_tag_set(
                            rejected_osm_id,
                            10.1000,
                            20.1000,
                            32_001,
                            10.1010,
                            20.1010,
                            0,
                        ),
                        line_record_with_tag_set(
                            rejected_osm_id,
                            10.1000,
                            20.1000,
                            32_002,
                            10.1020,
                            20.1020,
                            1,
                        ),
                        line_record_with_tag_set(
                            accepted_osm_id,
                            10.1030,
                            20.1000,
                            32_101,
                            10.1040,
                            20.1010,
                            0,
                        ),
                    ],
                    line_refs: vec![0, 1, 2],
                    tag_values,
                    tag_sets,
                    rules: Vec::new(),
                    rule_line_refs: Vec::new(),
                },
            );

            let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
            let closest = manager
                .get_closest_to_coords(10.1001, 20.1000, &rules, false, None)
                .unwrap();

            assert_eq!(
                closest,
                Some((fixture.tile_id, accepted_osm_id)),
                "{label} avoid rule should reject the mixed-adjacency nearest point",
            );

            fs::remove_dir_all(fixture.dir).unwrap();
        }
    }

    #[test]
    fn test_get_closest_to_coords_residential_avoidance_still_applies_after_highway_filtering() {
        let residential_osm_id = 33_000;
        let allowed_osm_id = 33_100;
        let fixture = create_closest_point_fixture(
            "tile-manager-residential-and-highway-filtering",
            TileSpec {
                tile_id: SYNTHETIC_TILE_ID,
                bounds: SYNTHETIC_TILE_BOUNDS,
                spatial_index: Vec::new(),
                points: vec![
                    point_record_with_flags(
                        residential_osm_id,
                        10.1000,
                        20.1000,
                        0,
                        1,
                        PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG,
                    ),
                    point_record(allowed_osm_id, 10.1020, 20.1000, 1, 1),
                ],
                lines: vec![
                    line_record_with_tag_set(
                        residential_osm_id,
                        10.1000,
                        20.1000,
                        33_001,
                        10.1010,
                        20.1010,
                        0,
                    ),
                    line_record_with_tag_set(
                        allowed_osm_id,
                        10.1020,
                        20.1000,
                        33_101,
                        10.1030,
                        20.1010,
                        0,
                    ),
                ],
                line_refs: vec![0, 1],
                tag_values: vec!["secondary".to_string()],
                tag_sets: vec![tag_set_record(Some(0), None, None)],
                rules: Vec::new(),
                rule_line_refs: Vec::new(),
            },
        );

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let closest = manager
            .get_closest_to_coords(
                10.1001,
                20.1000,
                &RouterRules::default(),
                true,
                Some(&["secondary"]),
            )
            .unwrap();

        assert_eq!(closest, Some((fixture.tile_id, allowed_osm_id)));

        fs::remove_dir_all(fixture.dir).unwrap();
    }

    #[test]
    fn test_single_tile_query() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let mut manager = TileManager::new(tile_dir).unwrap();

        // Query for a point known to exist in Montenegro
        let point = manager
            .get_closest_to_coords(42.5, 18.5, &RouterRules::default(), false, None)
            .unwrap();
        assert!(point.is_some());
    }

    #[test]
    fn test_cross_tile_traversal_with_synthetic_fixture() {
        let SyntheticCrossTileFixture {
            dir,
            tile_a,
            tile_b,
            center_osm_id,
            in_tile_neighbor_osm_id,
            cross_tile_neighbor_osm_id,
        } = create_cross_tile_fixture("tile-manager-cross-tile-synthetic");

        let mut manager = TileManager::new(dir.clone()).unwrap();
        let adjacent = manager.get_adjacent_by_id(tile_a, center_osm_id).unwrap();

        assert_eq!(adjacent.len(), 2);
        assert!(adjacent.contains(&(tile_a, 0, tile_a, in_tile_neighbor_osm_id)));
        assert!(adjacent.contains(&(tile_a, 1, tile_b, cross_tile_neighbor_osm_id)));
        assert_eq!(manager.loaded_tile_count(), 2);
        assert_eq!(manager.loaded_tile_order_for_test(), vec![tile_a, tile_b]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_by_id_if_loaded_returns_none_when_center_tile_is_cold() {
        let SyntheticCrossTileFixture {
            dir,
            tile_a,
            center_osm_id,
            ..
        } = create_cross_tile_fixture("tile-manager-adjacent-if-loaded-cold-center");

        let manager = TileManager::new(dir.clone()).unwrap();

        let adjacent = manager
            .get_adjacent_by_id_if_loaded(tile_a, center_osm_id)
            .unwrap();

        assert_eq!(adjacent, None);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_by_id_if_loaded_returns_none_when_cross_tile_neighbor_is_cold() {
        let SyntheticCrossTileFixture {
            dir,
            tile_a,
            center_osm_id,
            ..
        } = create_cross_tile_fixture("tile-manager-adjacent-if-loaded-cold-neighbor");

        let mut manager = TileManager::new(dir.clone()).unwrap();
        manager.ensure_tile_loaded(tile_a).unwrap();

        let adjacent = manager
            .get_adjacent_by_id_if_loaded(tile_a, center_osm_id)
            .unwrap();

        assert_eq!(adjacent, None);
        assert_eq!(manager.loaded_tile_count(), 1);
        assert_eq!(manager.loaded_tile_order_for_test(), vec![tile_a]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_get_adjacent_by_id_if_loaded_returns_some_once_required_tiles_are_warm() {
        let SyntheticCrossTileFixture {
            dir,
            tile_a,
            tile_b,
            center_osm_id,
            in_tile_neighbor_osm_id,
            cross_tile_neighbor_osm_id,
        } = create_cross_tile_fixture("tile-manager-adjacent-if-loaded-warm");

        let mut manager = TileManager::new(dir.clone()).unwrap();
        manager.ensure_tile_loaded(tile_a).unwrap();
        manager.ensure_tile_loaded(tile_b).unwrap();

        let adjacent = manager
            .get_adjacent_by_id_if_loaded(tile_a, center_osm_id)
            .unwrap()
            .unwrap();

        assert_eq!(adjacent.len(), 2);
        assert!(adjacent.contains(&(tile_a, 0, tile_a, in_tile_neighbor_osm_id)));
        assert!(adjacent.contains(&(tile_a, 1, tile_b, cross_tile_neighbor_osm_id)));
        assert_eq!(manager.loaded_tile_count(), 2);
        assert_eq!(manager.loaded_tile_order_for_test(), vec![tile_a, tile_b]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_cross_tile_traversal() {
        let tile_dir = PathBuf::from("test_data/montenegro_tiles");

        // Skip if test data doesn't exist
        if !tile_dir.exists() {
            eprintln!("Skipping test: test_data/montenegro_tiles doesn't exist");
            return;
        }

        let mut manager = TileManager::new(tile_dir).unwrap();

        // Get a point near a tile boundary
        let point = manager
            .get_closest_to_coords(42.1, 18.9, &RouterRules::default(), false, None)
            .unwrap()
            .unwrap();

        // Get adjacent points (may cross tile boundary)
        let adjacent = manager.get_adjacent_by_id(point.0, point.1).unwrap();

        // Should have at least one adjacent point
        assert!(adjacent.len() > 0);

        // Check if any cross tile boundary
        let crosses_boundary = adjacent
            .iter()
            .any(|(line_tile_id, _, other_tile_id, _)| *line_tile_id != *other_tile_id);
        // May or may not cross depending on location
        let _ = crosses_boundary;
    }

    #[test]
    fn test_missing_tile_handling() {
        let fixture = create_missing_neighbor_fixture("tile-manager-missing-neighbor");
        let fixture_dir = fixture.dir.clone();

        let mut manager = TileManager::new(fixture.dir.clone()).unwrap();
        let adjacent = manager.get_adjacent_by_id(fixture.tile_a, fixture.center_osm_id);

        assert!(
            adjacent.is_ok(),
            "missing-neighbor adjacency lookup should succeed: {adjacent:?}",
        );

        let adjacent = adjacent.unwrap();
        assert_eq!(
            adjacent.len(),
            1,
            "missing tile edge should be filtered out"
        );
        assert!(adjacent.contains(&(
            fixture.tile_a,
            0,
            fixture.tile_a,
            fixture.in_tile_neighbor_osm_id,
        )));
        assert!(!adjacent.iter().any(|(_, _, other_tile_id, other_osm_id)| {
            *other_tile_id == fixture.missing_tile
                || *other_osm_id == fixture.missing_neighbor_osm_id
        }));
        assert_eq!(manager.loaded_tile_count(), 1);
        assert_eq!(manager.loaded_tile_order_for_test(), vec![fixture.tile_a]);

        std::fs::remove_dir_all(fixture_dir).unwrap();
    }
}
