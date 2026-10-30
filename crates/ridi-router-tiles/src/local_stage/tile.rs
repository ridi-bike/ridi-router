//! Per-tile graph build + RMDF serialization, index based.
//!
//! Produces byte-identical files to `GenerationGraph` + `RmdfWriter::write_tile_from_graph`
//! (same point/line/tag/rule order), without per-tile HashMaps over OSM ids, without cloning
//! relations, and hashing the file in memory instead of re-reading it.

use anyhow::Result;
use bytemuck::bytes_of;
use sha2::{Digest, Sha256};

use ridi_router_common::manifest::{TileBounds as ManifestBounds, TileMetadata};
use ridi_router_common::osm::OsmRelation;

use super::{NodeTable, WayTable};
use crate::generation::{
    materialize_restriction, GenerationRestrictionRule, GenerationRestrictionRuleType,
    GenerationRestrictionSkipStats, GenerationTags, LineDirection, RestrictionContext,
};
use crate::rmdf::format::*;

/// Profiling span (hotpath feature only); drop the returned value to end it.
#[cfg(feature = "hotpath")]
macro_rules! span {
    ($name:expr) => {
        Some(hotpath::functions::build_measurement_guard_sync(
            $name, false,
        ))
    };
}
#[cfg(not(feature = "hotpath"))]
macro_rules! span {
    ($name:expr) => {
        None::<NoSpan>
    };
}

#[cfg(not(feature = "hotpath"))]
struct NoSpan;

struct LocalLine {
    from: u32, // local point index
    to: u32,
    direction: LineDirection,
    tag_set: u32,
}

struct TileGraph<'a> {
    nodes: &'a NodeTable<'a>,
    /// global node indices, ascending (= ascending OSM id)
    points: &'a [u32],
    lines: Vec<LocalLine>,
    /// (way id, first line, end line) ascending by way id
    way_lines: Vec<(u64, u32, u32)>,
}

impl TileGraph<'_> {
    #[inline]
    fn local_of_global(&self, global: u32) -> Option<u32> {
        if global == u32::MAX {
            return None;
        }
        self.points.binary_search(&global).ok().map(|i| i as u32)
    }

    #[inline]
    fn point_id(&self, local: u32) -> u64 {
        self.nodes.ids[self.points[local as usize] as usize]
    }
}

impl RestrictionContext for TileGraph<'_> {
    fn has_point(&self, node_id: u64) -> bool {
        self.points
            .binary_search_by_key(&node_id, |&g| self.nodes.ids[g as usize])
            .is_ok()
    }

    fn via_adjacent_lines_for_way(&self, way_id: u64, via_node_id: u64) -> Vec<u32> {
        let Ok(pos) = self.way_lines.binary_search_by_key(&way_id, |w| w.0) else {
            return Vec::new();
        };
        let (_, start, end) = self.way_lines[pos];
        (start..end)
            .filter(|&li| {
                let line = &self.lines[li as usize];
                self.point_id(line.from) == via_node_id || self.point_id(line.to) == via_node_id
            })
            .collect()
    }
}

pub(crate) struct TileResult {
    pub metadata: TileMetadata,
    /// Complete file contents (incl. checksum); written by the caller.
    pub bytes: Vec<u8>,
}

fn tile_bounds(tile_id: TileId, tile_size_degrees: f32) -> TileBounds {
    let lon_min = (tile_id.col as f32 * tile_size_degrees) - 180.0;
    let lon_max = lon_min + tile_size_degrees;
    let lat_min = (tile_id.row as f32 * tile_size_degrees) - 90.0;
    let lat_max = lat_min + tile_size_degrees;
    TileBounds {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
    }
}

/// Build one tile (file bytes + manifest entry). `points` / `ways` are ascending global indices; `restrictions`
/// ascending by relation id. Node flags: bit0 residential, bit1 nogo.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_tile(
    tile_id: TileId,
    tile_size_degrees: f32,
    nodes: &NodeTable<'_>,
    node_flags: &[u8],
    ways: &WayTable<'_>,
    points: &[u32],
    tile_ways: &[u32],
    restrictions: &[&OsmRelation],
    skip_stats: &mut GenerationRestrictionSkipStats,
) -> Result<TileResult> {
    // ---- graph
    let g = span!("tile.graph");
    let mut graph = TileGraph {
        nodes,
        points,
        lines: Vec::new(),
        way_lines: Vec::with_capacity(tile_ways.len()),
    };
    let mut tags = GenerationTags::new();
    for &w in tile_ways {
        let w = w as usize;
        let refs = ways.refs_of(w);
        if refs.len() < 2 {
            continue;
        }
        let way_tags = &ways.tag_sets[ways.tag_set[w] as usize];
        let tag_set_id = tags.get_or_create(
            way_tags.name.as_ref(),
            way_tags.hw_ref.as_ref(),
            way_tags.highway.as_ref(),
            way_tags.surface.as_ref(),
            way_tags.smoothness.as_ref(),
        );
        let direction = way_tags.direction();
        let start = graph.lines.len() as u32;
        let mut prev = graph.local_of_global(refs[0]);
        for &r in &refs[1..] {
            let cur = graph.local_of_global(r);
            if let (Some(a), Some(b)) = (prev, cur) {
                graph.lines.push(LocalLine {
                    from: a,
                    to: b,
                    direction,
                    tag_set: tag_set_id.tag_set_idx,
                });
            }
            prev = cur;
        }
        let end = graph.lines.len() as u32;
        if end > start {
            graph.way_lines.push((ways.ids[w], start, end));
        }
    }

    drop(g);
    // ---- restrictions: (via node id, rule) in insertion order
    let g = span!("tile.restrictions");
    let mut rules: Vec<(u64, GenerationRestrictionRule)> = Vec::new();
    for relation in restrictions {
        if let Some(rule) = materialize_restriction(&graph, relation, skip_stats, false) {
            rules.push((rule.via_node_id, rule));
        }
    }
    rules.sort_by_key(|(via, _)| *via); // stable: keeps insertion order per via

    drop(g);
    // ---- point layout
    let g = span!("tile.layout");
    let n_points = points.len();
    let mut line_counts = vec![0u32; n_points];
    for line in &graph.lines {
        line_counts[line.from as usize] += 1;
        line_counts[line.to as usize] += 1;
    }
    let mut point_line_offsets = vec![0u32; n_points + 1];
    for i in 0..n_points {
        point_line_offsets[i + 1] = point_line_offsets[i] + line_counts[i];
    }
    let mut fill = point_line_offsets.clone();
    let mut point_lines = vec![0u32; point_line_offsets[n_points] as usize];
    for (li, line) in graph.lines.iter().enumerate() {
        point_lines[fill[line.from as usize] as usize] = li as u32;
        fill[line.from as usize] += 1;
        point_lines[fill[line.to as usize] as usize] = li as u32;
        fill[line.to as usize] += 1;
    }

    let point_lat_lon = |local: u32| -> (f32, f32) {
        let g = points[local as usize] as usize;
        (
            (nodes.lat_e7[g] as f64 * 1e-7) as f32,
            (nodes.lon_e7[g] as f64 * 1e-7) as f32,
        )
    };

    // connected points ordered by (cell id, osm id)
    let mut ordered: Vec<(u32, u32)> = (0..n_points as u32)
        .filter(|&l| line_counts[l as usize] > 0)
        .map(|l| {
            let (lat, lon) = point_lat_lon(l);
            (GridCellEntry::encode_cell_id(lat, lon, 100), l)
        })
        .collect();
    ordered.sort_by_key(|&(cell, l)| (cell, l));

    drop(g);
    // ---- sections
    let g = span!("tile.serialize");
    let mut spatial_index: Vec<u8> = Vec::new();
    let mut points_bytes: Vec<u8> =
        Vec::with_capacity(ordered.len() * std::mem::size_of::<PointRecord>());
    let mut line_refs: Vec<u8> = Vec::with_capacity(point_lines.len() * 8);
    let mut rule_records: Vec<RuleRecord> = Vec::new();
    let mut rule_line_refs: Vec<u64> = Vec::new();

    let mut cell_run: Option<(u32, u64, u32)> = None;
    let mut line_offset = 0u64;
    let mut rule_offset = 0u64;
    for (point_idx, &(cell_id, l)) in ordered.iter().enumerate() {
        match &mut cell_run {
            Some((cell, _, count)) if *cell == cell_id => *count += 1,
            _ => {
                if let Some((cell, offset, count)) = cell_run.take() {
                    spatial_index.extend_from_slice(bytes_of(&GridCellEntry {
                        cell_id: cell,
                        _padding1: 0,
                        points_offset: offset,
                        points_count: count,
                        _padding2: 0,
                    }));
                }
                cell_run = Some((cell_id, point_idx as u64, 1));
            }
        }

        let id = graph.point_id(l);
        let (lat, lon) = point_lat_lon(l);
        let lines_count = line_counts[l as usize];
        let start = rules.partition_point(|(via, _)| *via < id);
        let end = start
            + rules[start..]
                .iter()
                .take_while(|(via, _)| *via == id)
                .count();
        let rules_count = (end - start) as u32;
        let flags_bits = node_flags[points[l as usize] as usize];
        let mut flags = 0u16;
        if flags_bits & 1 != 0 {
            flags |= PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
        }
        if flags_bits & 2 != 0 {
            flags |= PointRecord::NOGO_AREA_FLAG;
        }
        points_bytes.extend_from_slice(bytes_of(&PointRecord {
            osm_id: id,
            lat,
            lon,
            lines_offset: line_offset,
            lines_count,
            _padding1: 0,
            rules_offset: rule_offset,
            rules_count,
            flags,
            _padding2: 0,
        }));
        for &li in &point_lines
            [point_line_offsets[l as usize] as usize..point_line_offsets[l as usize + 1] as usize]
        {
            line_refs.extend_from_slice(&(li as u64).to_le_bytes());
        }
        line_offset += lines_count as u64;
        rule_offset += rules_count as u64;

        // rules for this point (serialized in ordered point order)
        for (_, rule) in &rules[start..end] {
            let from_lines_offset = rule_line_refs.len() as u64;
            rule_line_refs.extend(rule.from_line_indices.iter().map(|&i| i as u64));
            let to_lines_offset = rule_line_refs.len() as u64;
            rule_line_refs.extend(rule.to_line_indices.iter().map(|&i| i as u64));
            rule_records.push(RuleRecord {
                from_lines_offset,
                from_lines_count: rule.from_line_indices.len() as u32,
                _padding1: 0,
                to_lines_offset,
                to_lines_count: rule.to_line_indices.len() as u32,
                rule_type: match rule.rule_type {
                    GenerationRestrictionRuleType::OnlyAllowed => 0,
                    GenerationRestrictionRuleType::NotAllowed => 1,
                },
                _padding2: 0,
                _padding3: 0,
            });
        }
    }
    if let Some((cell, offset, count)) = cell_run.take() {
        spatial_index.extend_from_slice(bytes_of(&GridCellEntry {
            cell_id: cell,
            _padding1: 0,
            points_offset: offset,
            points_count: count,
            _padding2: 0,
        }));
    }

    let mut lines_bytes: Vec<u8> =
        Vec::with_capacity(graph.lines.len() * std::mem::size_of::<LineRecord>());
    for line in &graph.lines {
        let (a_lat, a_lon) = point_lat_lon(line.from);
        let (b_lat, b_lon) = point_lat_lon(line.to);
        lines_bytes.extend_from_slice(bytes_of(&LineRecord {
            point_a_osm_id: graph.point_id(line.from),
            point_a_lat: a_lat,
            point_a_lon: a_lon,
            point_b_osm_id: graph.point_id(line.to),
            point_b_lat: b_lat,
            point_b_lon: b_lon,
            direction: match line.direction {
                LineDirection::BothWays => 0,
                LineDirection::OneWay => 1,
                LineDirection::Roundabout => 2,
            },
            _padding1: 0,
            _padding2: 0,
            tag_set_index: line.tag_set,
        }));
    }

    let mut tag_values: Vec<u8> = Vec::new();
    let mut tag_strings: Vec<u8> = Vec::new();
    for value in &tags.tag_values {
        tag_values.extend_from_slice(bytes_of(&StringEntry {
            offset: tag_strings.len() as u64,
            length: value.len() as u32,
            _padding: 0,
        }));
        tag_strings.extend_from_slice(value.as_bytes());
    }
    let mut tag_sets: Vec<u8> = Vec::new();
    for set in &tags.tag_sets {
        tag_sets.extend_from_slice(bytes_of(&TagSetRecord {
            name_idx: set.name.tag_value_idx,
            hw_ref_idx: set.hw_ref.tag_value_idx,
            highway_idx: set.highway.tag_value_idx,
            surface_idx: set.surface.tag_value_idx,
            smoothness_idx: set.smoothness.tag_value_idx,
        }));
    }
    let mut rules_bytes: Vec<u8> = Vec::new();
    for record in &rule_records {
        rules_bytes.extend_from_slice(bytes_of(record));
    }
    for r in &rule_line_refs {
        rules_bytes.extend_from_slice(&r.to_le_bytes());
    }

    // ---- header + file
    let mut offset = RmdfHeader::SIZE;
    let mut section_offsets = [0u64; NUM_SECTIONS];
    section_offsets[section::SPATIAL_INDEX] = offset as u64;
    offset += spatial_index.len();
    section_offsets[section::POINTS] = offset as u64;
    offset += points_bytes.len();
    section_offsets[section::LINES] = offset as u64;
    offset += lines_bytes.len();
    section_offsets[section::LINE_REFS] = offset as u64;
    offset += line_refs.len();
    section_offsets[section::TAG_VALUES] = offset as u64;
    offset += tag_values.len() + tag_strings.len();
    section_offsets[section::TAG_SETS] = offset as u64;
    offset += tag_sets.len();
    section_offsets[section::RULES] = offset as u64;
    offset += rules_bytes.len();

    let bounds = tile_bounds(tile_id, tile_size_degrees);
    let header = RmdfHeader {
        magic: RmdfHeader::MAGIC,
        version: RmdfHeader::VERSION,
        tile_bounds: bounds,
        point_count: ordered.len() as u64,
        line_count: graph.lines.len() as u64,
        spatial_grid_cell_count: (spatial_index.len() / std::mem::size_of::<GridCellEntry>())
            as u32,
        tag_value_count: (tag_values.len() / std::mem::size_of::<StringEntry>()) as u32,
        tag_set_count: (tag_sets.len() / std::mem::size_of::<TagSetRecord>()) as u32,
        rule_count: rule_records.len() as u32,
        section_offsets,
    };

    let mut file = Vec::with_capacity(offset + 32);
    file.extend_from_slice(bytes_of(&header));
    file.extend_from_slice(&spatial_index);
    file.extend_from_slice(&points_bytes);
    file.extend_from_slice(&lines_bytes);
    file.extend_from_slice(&line_refs);
    file.extend_from_slice(&tag_values);
    file.extend_from_slice(&tag_strings);
    file.extend_from_slice(&tag_sets);
    file.extend_from_slice(&rules_bytes);
    drop(g);
    let g = span!("tile.sha256");
    let checksum = Sha256::digest(&file);
    file.extend_from_slice(&checksum);

    let filename = tile_id.to_filename();
    drop(g);

    Ok(TileResult {
        metadata: TileMetadata {
            filename,
            col: tile_id.col,
            row: tile_id.row,
            bounds: ManifestBounds {
                lat_min: bounds.lat_min,
                lat_max: bounds.lat_max,
                lon_min: bounds.lon_min,
                lon_max: bounds.lon_max,
            },
            neighbors: super::manifest::empty_neighbors(),
            size_bytes: file.len() as u64,
            point_count: header.point_count,
            line_count: header.line_count,
            checksum: format!("sha256:{}", hex::encode(checksum)),
            military_geojson_filename: None,
        },
        bytes: file,
    })
}
