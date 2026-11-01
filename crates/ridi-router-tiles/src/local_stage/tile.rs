//! Per-tile graph build + RMDF serialization, index based.
//!
//! Builds each tile without per-tile HashMaps over OSM ids or cloned relations, and checksums
//! the file in memory instead of re-reading it.

use anyhow::{ensure, Result};
use bytemuck::{bytes_of, cast_slice};

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
    // local point index -> index in the points section (lines refer to points by the latter)
    let mut written = vec![u32::MAX; n_points];
    for (i, &(_, l)) in ordered.iter().enumerate() {
        written[l as usize] = i as u32;
    }

    drop(g);
    // ---- sections
    let g = span!("tile.serialize");
    let mut spatial_index: Vec<GridCellEntry> = Vec::new();
    let mut point_records: Vec<PointRecord> = Vec::with_capacity(ordered.len());
    let mut line_refs: Vec<u32> = Vec::with_capacity(point_lines.len());
    let mut rule_records: Vec<RuleRecord> = Vec::new();
    let mut rule_line_refs: Vec<u32> = Vec::new();

    let mut cell_run: Option<(u32, u32, u32)> = None;
    for (point_idx, &(cell_id, l)) in ordered.iter().enumerate() {
        match &mut cell_run {
            Some((cell, _, count)) if *cell == cell_id => *count += 1,
            _ => {
                if let Some((cell, offset, count)) = cell_run.take() {
                    spatial_index.push(GridCellEntry {
                        cell_id: cell,
                        points_offset: offset,
                        points_count: count,
                    });
                }
                cell_run = Some((cell_id, point_idx as u32, 1));
            }
        }

        let id = graph.point_id(l);
        let (lat, lon) = point_lat_lon(l);
        let lines_count = line_counts[l as usize];
        ensure!(
            lines_count <= u16::MAX as u32,
            "point {id} has {lines_count} lines, more than a point record can hold"
        );
        let flags_bits = node_flags[points[l as usize] as usize];
        let mut flags = 0u16;
        if flags_bits & 1 != 0 {
            flags |= PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
        }
        if flags_bits & 2 != 0 {
            flags |= PointRecord::NOGO_AREA_FLAG;
        }
        point_records.push(PointRecord {
            osm_id: id,
            lat,
            lon,
            lines_offset: line_refs.len() as u32,
            lines_count: lines_count as u16,
            flags,
        });
        line_refs.extend_from_slice(
            &point_lines[point_line_offsets[l as usize] as usize
                ..point_line_offsets[l as usize + 1] as usize],
        );
    }
    if let Some((cell, offset, count)) = cell_run.take() {
        spatial_index.push(GridCellEntry {
            cell_id: cell,
            points_offset: offset,
            points_count: count,
        });
    }

    // rules are already sorted by via node; keep those whose via node is a written point
    for (via, rule) in &rules {
        let Ok(via_idx) = graph
            .points
            .binary_search_by_key(via, |&g| nodes.ids[g as usize])
        else {
            continue;
        };
        let point_idx = written[via_idx];
        if point_idx == u32::MAX {
            continue;
        }
        point_records[point_idx as usize].flags |= PointRecord::HAS_RULES_FLAG;
        ensure!(
            rule.from_line_indices.len() <= u16::MAX as usize
                && rule.to_line_indices.len() <= u16::MAX as usize,
            "restriction via {via} has too many lines for a rule record"
        );
        let from_lines_offset = rule_line_refs.len() as u32;
        rule_line_refs.extend_from_slice(&rule.from_line_indices);
        let to_lines_offset = rule_line_refs.len() as u32;
        rule_line_refs.extend_from_slice(&rule.to_line_indices);
        rule_records.push(RuleRecord {
            via_osm_id: *via,
            from_lines_offset,
            to_lines_offset,
            from_lines_count: rule.from_line_indices.len() as u16,
            to_lines_count: rule.to_line_indices.len() as u16,
            rule_type: match rule.rule_type {
                GenerationRestrictionRuleType::OnlyAllowed => 0,
                GenerationRestrictionRuleType::NotAllowed => 1,
            },
            _padding: [0; 3],
        });
    }

    let mut line_records: Vec<LineRecord> = Vec::with_capacity(graph.lines.len());
    for line in &graph.lines {
        line_records.push(LineRecord::new(
            written[line.from as usize],
            written[line.to as usize],
            line.tag_set,
            match line.direction {
                LineDirection::BothWays => 0,
                LineDirection::OneWay => 1,
                LineDirection::Roundabout => 2,
            },
        ));
    }

    let mut tag_values: Vec<StringEntry> = Vec::with_capacity(tags.tag_values.len());
    let mut tag_strings: Vec<u8> = Vec::new();
    for value in &tags.tag_values {
        tag_values.push(StringEntry {
            offset: tag_strings.len() as u32,
            length: value.len() as u32,
        });
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

    // ---- header + file
    let tag_value_bytes: &[u8] = cast_slice(&tag_values);
    let sections: [&[&[u8]]; NUM_SECTIONS] = [
        &[cast_slice(&spatial_index)],
        &[cast_slice(&point_records)],
        &[cast_slice(&line_records)],
        &[cast_slice(&line_refs)],
        &[tag_value_bytes, &tag_strings],
        &[&tag_sets],
        &[cast_slice(&rule_records), cast_slice(&rule_line_refs)],
    ];
    let mut offset = RmdfHeader::SIZE;
    let mut section_offsets = [0u64; NUM_SECTIONS];
    for (i, parts) in sections.iter().enumerate() {
        offset = align_section(offset);
        section_offsets[i] = offset as u64;
        offset += parts.iter().map(|p| p.len()).sum::<usize>();
    }

    let bounds = tile_bounds(tile_id, tile_size_degrees);
    let header = RmdfHeader {
        magic: RmdfHeader::MAGIC,
        version: RmdfHeader::VERSION,
        tile_bounds: bounds,
        point_count: point_records.len() as u64,
        line_count: graph.lines.len() as u64,
        spatial_grid_cell_count: spatial_index.len() as u32,
        tag_value_count: tag_values.len() as u32,
        tag_set_count: (tag_sets.len() / std::mem::size_of::<TagSetRecord>()) as u32,
        rule_count: rule_records.len() as u32,
        section_offsets,
    };

    let mut file = Vec::with_capacity(offset + CHECKSUM_LEN);
    file.extend_from_slice(bytes_of(&header));
    for (i, parts) in sections.iter().enumerate() {
        file.resize(section_offsets[i] as usize, 0);
        for part in *parts {
            file.extend_from_slice(part);
        }
    }
    drop(g);
    let g = span!("tile.checksum");
    let checksum = checksum(&file);
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
            checksum: checksum_string(&checksum),
            military_geojson_filename: None,
        },
        bytes: file,
    })
}
