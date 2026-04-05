use anyhow::{Context, Result};
use bytemuck::bytes_of;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use tracing::debug;

use crate::generation::{
    GenerationError, GenerationGraph, GenerationPoint, GenerationRestrictionRuleType, LineDirection,
};
use crate::rmdf::format::*;

struct OrderedPoint<'a> {
    cell_id: u32,
    point: &'a GenerationPoint,
    line_indices: Vec<u64>,
    lines_offset: u64,
    lines_count: u32,
    rules_offset: u64,
    rules_count: u32,
}

struct PointLayout<'a> {
    ordered_points: Vec<OrderedPoint<'a>>,
}

struct SerializedRules {
    bytes: Vec<u8>,
    rule_record_count: u32,
}

pub struct RmdfWriter {
    tile_size_degrees: f32,
}

impl RmdfWriter {
    pub fn new(tile_size_degrees: f32) -> Self {
        Self { tile_size_degrees }
    }

    /// Build a mapping from point OSM IDs to the indices of lines that connect to them.
    /// The generation model keeps line relationships explicit on lines, not on points.
    fn build_point_lines_map(graph: &GenerationGraph) -> HashMap<u64, Vec<usize>> {
        let mut map: HashMap<u64, Vec<usize>> = HashMap::new();

        for (line_idx, line) in graph.get_lines().iter().enumerate() {
            map.entry(line.from_node_id).or_default().push(line_idx);
            map.entry(line.to_node_id).or_default().push(line_idx);
        }

        map
    }

    fn collect_ordered_connected_points<'a>(
        graph: &'a GenerationGraph,
        point_lines_map: &HashMap<u64, Vec<usize>>,
    ) -> Vec<(u32, &'a GenerationPoint)> {
        let mut cells: BTreeMap<u32, Vec<&GenerationPoint>> = BTreeMap::new();

        for point in graph.get_points() {
            if !point_lines_map.contains_key(&point.id) {
                continue;
            }

            let cell_id = GridCellEntry::encode_cell_id(point.lat, point.lon, 100);
            cells.entry(cell_id).or_default().push(point);
        }

        let mut ordered_points = Vec::new();
        for (cell_id, mut points_in_cell) in cells {
            points_in_cell.sort_unstable_by_key(|point| point.id);
            for point in points_in_cell {
                ordered_points.push((cell_id, point));
            }
        }

        ordered_points
    }

    fn build_point_layout<'a>(graph: &'a GenerationGraph) -> Result<PointLayout<'a>> {
        let point_lines_map = Self::build_point_lines_map(graph);
        let ordered_points = Self::collect_ordered_connected_points(graph, &point_lines_map);
        let restrictions_by_via = graph.get_restrictions_by_via();

        let mut line_offset = 0u64;
        let mut rule_offset = 0u64;
        let mut layout = Vec::with_capacity(ordered_points.len());

        for (cell_id, point) in ordered_points {
            let point_line_indices = point_lines_map
                .get(&point.id)
                .expect("ordered connected point must have line refs");
            let line_indices: Vec<u64> = point_line_indices.iter().map(|&idx| idx as u64).collect();
            let lines_count = u32::try_from(line_indices.len())
                .context("point line refs count must fit in u32")?;

            let rules_count = restrictions_by_via
                .get(&point.id)
                .map(|rules| u32::try_from(rules.len()))
                .transpose()
                .context("point rule count must fit in u32")?
                .unwrap_or(0);

            layout.push(OrderedPoint {
                cell_id,
                point,
                line_indices,
                lines_offset: line_offset,
                lines_count,
                rules_offset: rule_offset,
                rules_count,
            });

            line_offset += u64::from(lines_count);
            rule_offset += u64::from(rules_count);
        }

        Ok(PointLayout {
            ordered_points: layout,
        })
    }

    /// Write RMDF tile directly from GenerationGraph
    pub fn write_tile_from_graph(
        &self,
        tile_id: TileId,
        graph: GenerationGraph,
        output_path: &Path,
    ) -> Result<()> {
        debug!("Writing RMDF file from GenerationGraph: {:?}", output_path);

        let point_layout =
            Self::build_point_layout(&graph).context("Failed to build point layout")?;

        // Serialize all sections
        let spatial_index = self
            .build_spatial_index(&point_layout)
            .context("Failed to build spatial index")?;
        let points = self
            .serialize_points(&point_layout)
            .context("Failed to serialize points")?;
        let lines = self
            .serialize_lines(&graph)
            .context("Failed to serialize lines")?;
        let line_refs = self
            .serialize_line_refs(&point_layout)
            .context("Failed to serialize line refs")?;
        let (tag_values, tag_strings) = self
            .serialize_tag_values(&graph)
            .context("Failed to serialize tag values")?;
        let tag_sets = self
            .serialize_tag_sets(&graph)
            .context("Failed to serialize tag sets")?;
        let rules = self
            .serialize_rules(&graph, &point_layout)
            .context("Failed to serialize rules")?;

        // Calculate section offsets
        let mut offset = RmdfHeader::SIZE;
        let mut section_offsets = [0u64; NUM_SECTIONS];

        section_offsets[section::SPATIAL_INDEX] = offset as u64;
        offset += spatial_index.len();

        section_offsets[section::POINTS] = offset as u64;
        offset += points.len();

        section_offsets[section::LINES] = offset as u64;
        offset += lines.len();

        section_offsets[section::LINE_REFS] = offset as u64;
        offset += line_refs.len();

        section_offsets[section::TAG_VALUES] = offset as u64;
        offset += tag_values.len() + tag_strings.len();

        section_offsets[section::TAG_SETS] = offset as u64;
        offset += tag_sets.len();

        section_offsets[section::RULES] = offset as u64;

        // Build header
        let bounds = self.compute_tile_bounds(tile_id);
        let header = RmdfHeader {
            magic: RmdfHeader::MAGIC,
            version: RmdfHeader::VERSION,
            tile_bounds: bounds,
            point_count: point_layout.ordered_points.len() as u64,
            line_count: graph.get_lines().len() as u64,
            spatial_grid_cell_count: (spatial_index.len() / std::mem::size_of::<GridCellEntry>())
                as u32,
            tag_value_count: (tag_values.len() / std::mem::size_of::<StringEntry>()) as u32,
            tag_set_count: (tag_sets.len() / std::mem::size_of::<TagSetRecord>()) as u32,
            rule_count: rules.rule_record_count,
            section_offsets,
        };

        // Write all data to file
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_path)
            .context("Failed to create output file")?;

        file.write_all(bytes_of(&header))
            .context("Failed to write header")?;
        file.write_all(&spatial_index)
            .context("Failed to write spatial index")?;
        file.write_all(&points).context("Failed to write points")?;
        file.write_all(&lines).context("Failed to write lines")?;
        file.write_all(&line_refs)
            .context("Failed to write line refs")?;
        file.write_all(&tag_values)
            .context("Failed to write tag values")?;
        file.write_all(&tag_strings)
            .context("Failed to write tag strings")?;
        file.write_all(&tag_sets)
            .context("Failed to write tag sets")?;
        file.write_all(&rules.bytes)
            .context("Failed to write rules")?;

        // Compute and append checksum
        file.seek(SeekFrom::Start(0))
            .context("Failed to seek to start for checksum")?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).context("Failed to copy file for checksum")?;
        let checksum = hasher.finalize();

        file.write_all(&checksum)
            .context("Failed to write checksum")?;

        let file_size = file
            .metadata()
            .context("Failed to get file metadata")?
            .len();
        debug!("RMDF file written: {} bytes", file_size);

        Ok(())
    }

    fn build_spatial_index(&self, point_layout: &PointLayout<'_>) -> Result<Vec<u8>> {
        let mut grid_entries = Vec::new();
        let mut current_cell_id = None;
        let mut current_points_offset = 0u64;
        let mut current_points_count = 0u32;

        for (point_idx, ordered_point) in point_layout.ordered_points.iter().enumerate() {
            match current_cell_id {
                Some(cell_id) if cell_id == ordered_point.cell_id => {
                    current_points_count += 1;
                }
                Some(cell_id) => {
                    grid_entries.push(GridCellEntry {
                        cell_id,
                        _padding1: 0,
                        points_offset: current_points_offset,
                        points_count: current_points_count,
                        _padding2: 0,
                    });
                    current_cell_id = Some(ordered_point.cell_id);
                    current_points_offset = point_idx as u64;
                    current_points_count = 1;
                }
                None => {
                    current_cell_id = Some(ordered_point.cell_id);
                    current_points_offset = point_idx as u64;
                    current_points_count = 1;
                }
            }
        }

        if let Some(cell_id) = current_cell_id {
            grid_entries.push(GridCellEntry {
                cell_id,
                _padding1: 0,
                points_offset: current_points_offset,
                points_count: current_points_count,
                _padding2: 0,
            });
        }

        let bytes: Vec<u8> = grid_entries
            .iter()
            .flat_map(|entry| bytes_of(entry).to_vec())
            .collect();

        Ok(bytes)
    }

    fn serialize_points(&self, point_layout: &PointLayout<'_>) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        for ordered_point in &point_layout.ordered_points {
            let point = ordered_point.point;
            let record = PointRecord {
                osm_id: point.id,
                lat: point.lat,
                lon: point.lon,
                lines_offset: ordered_point.lines_offset,
                lines_count: ordered_point.lines_count,
                _padding1: 0,
                rules_offset: ordered_point.rules_offset,
                rules_count: ordered_point.rules_count,
                flags: {
                    let mut flags = 0u16;
                    if point.residential_in_proximity {
                        flags |= PointRecord::RESIDENTIAL_IN_PROXIMITY_FLAG;
                    }
                    if point.nogo_area {
                        flags |= PointRecord::NOGO_AREA_FLAG;
                    }
                    flags
                },
                _padding2: 0,
            };

            bytes.extend_from_slice(bytes_of(&record));
        }

        Ok(bytes)
    }

    fn serialize_lines(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        // Build a map of node IDs to points for quick lookup
        let mut node_map = std::collections::HashMap::new();
        for point in graph.get_points() {
            node_map.insert(point.id, point);
        }

        for line in graph.get_lines() {
            // Look up point data from node IDs
            let point_a =
                node_map
                    .get(&line.from_node_id)
                    .ok_or(GenerationError::MissingPoint {
                        point_id: line.from_node_id,
                    })?;
            let point_b = node_map
                .get(&line.to_node_id)
                .ok_or(GenerationError::MissingPoint {
                    point_id: line.to_node_id,
                })?;

            let record = LineRecord {
                point_a_osm_id: point_a.id,
                point_a_lat: point_a.lat,
                point_a_lon: point_a.lon,
                point_b_osm_id: point_b.id,
                point_b_lat: point_b.lat,
                point_b_lon: point_b.lon,
                direction: match line.direction {
                    LineDirection::BothWays => 0,
                    LineDirection::OneWay => 1,
                    LineDirection::Roundabout => 2,
                },
                _padding1: 0,
                _padding2: 0,
                tag_set_index: line.tags.tag_set_idx,
            };

            bytes.extend_from_slice(bytes_of(&record));
        }

        Ok(bytes)
    }

    fn serialize_line_refs(&self, point_layout: &PointLayout<'_>) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        for ordered_point in &point_layout.ordered_points {
            for line_idx in &ordered_point.line_indices {
                bytes.extend_from_slice(&line_idx.to_le_bytes());
            }
        }

        Ok(bytes)
    }

    fn serialize_tag_values(&self, graph: &GenerationGraph) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut entries = Vec::new();
        let mut string_pool = Vec::new();

        for tag_value in &graph.get_tags().tag_values {
            let offset = string_pool.len() as u64;
            let length = tag_value.len() as u32;

            let entry = StringEntry {
                offset,
                length,
                _padding: 0,
            };

            entries.extend_from_slice(bytes_of(&entry));
            string_pool.extend_from_slice(tag_value.as_bytes());
        }

        Ok((entries, string_pool))
    }

    fn serialize_tag_sets(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        for tag_set in &graph.get_tags().tag_sets {
            let record = TagSetRecord {
                name_idx: tag_set.name.tag_value_idx,
                hw_ref_idx: tag_set.hw_ref.tag_value_idx,
                highway_idx: tag_set.highway.tag_value_idx,
                surface_idx: tag_set.surface.tag_value_idx,
                smoothness_idx: tag_set.smoothness.tag_value_idx,
            };

            bytes.extend_from_slice(bytes_of(&record));
        }

        Ok(bytes)
    }

    fn serialize_rule_type(rule_type: &GenerationRestrictionRuleType) -> u8 {
        match rule_type {
            GenerationRestrictionRuleType::OnlyAllowed => 0,
            GenerationRestrictionRuleType::NotAllowed => 1,
        }
    }

    fn serialize_rules(
        &self,
        graph: &GenerationGraph,
        point_layout: &PointLayout<'_>,
    ) -> Result<SerializedRules> {
        let mut rule_records = Vec::new();
        let mut flattened_line_refs = Vec::new();

        for ordered_point in &point_layout.ordered_points {
            let Some(rules_for_point) =
                graph.get_restrictions_by_via().get(&ordered_point.point.id)
            else {
                continue;
            };

            for rule in rules_for_point {
                let from_lines_offset = flattened_line_refs.len() as u64;
                let from_lines_count = u32::try_from(rule.from_line_indices.len())
                    .context("rule from-line count must fit in u32")?;
                flattened_line_refs
                    .extend(rule.from_line_indices.iter().map(|&idx| u64::from(idx)));

                let to_lines_offset = flattened_line_refs.len() as u64;
                let to_lines_count = u32::try_from(rule.to_line_indices.len())
                    .context("rule to-line count must fit in u32")?;
                flattened_line_refs.extend(rule.to_line_indices.iter().map(|&idx| u64::from(idx)));

                rule_records.push(RuleRecord {
                    from_lines_offset,
                    from_lines_count,
                    _padding1: 0,
                    to_lines_offset,
                    to_lines_count,
                    rule_type: Self::serialize_rule_type(&rule.rule_type),
                    _padding2: 0,
                    _padding3: 0,
                });
            }
        }

        let rule_record_count =
            u32::try_from(rule_records.len()).context("rule record count must fit in u32")?;

        let mut bytes = Vec::new();
        for rule_record in &rule_records {
            bytes.extend_from_slice(bytes_of(rule_record));
        }
        for line_ref in &flattened_line_refs {
            bytes.extend_from_slice(&line_ref.to_le_bytes());
        }

        Ok(SerializedRules {
            bytes,
            rule_record_count,
        })
    }

    fn compute_tile_bounds(&self, tile_id: TileId) -> TileBounds {
        let lon_min = (tile_id.col as f32 * self.tile_size_degrees) - 180.0;
        let lon_max = lon_min + self.tile_size_degrees;
        let lat_min = (tile_id.row as f32 * self.tile_size_degrees) - 90.0;
        let lat_max = lat_min + self.tile_size_degrees;

        TileBounds {
            lat_min,
            lat_max,
            lon_min,
            lon_max,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bytemuck::{pod_read_unaligned, try_from_bytes, Pod};

    use super::*;
    use crate::generation::GenerationRestrictionRuleType;
    use crate::osm_data::{
        OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType,
        OsmWay,
    };

    #[test]
    fn test_serialize_points_writes_real_line_and_rule_offsets() {
        let writer = RmdfWriter::new(1.0);
        let graph = writer_test_graph();
        let point_layout = RmdfWriter::build_point_layout(&graph).unwrap();

        let point_records =
            parse_pod_records::<PointRecord>(&writer.serialize_points(&point_layout).unwrap());

        assert_eq!(point_records.len(), 6);
        assert_eq!(
            point_records
                .iter()
                .map(|point| point.osm_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );

        assert_eq!(point_records[0].lines_offset, 0);
        assert_eq!(point_records[0].lines_count, 1);
        assert_eq!(point_records[0].rules_count, 0);

        assert_eq!(point_records[1].lines_offset, 1);
        assert_eq!(point_records[1].lines_count, 4);
        assert_eq!(point_records[1].rules_offset, 0);
        assert_eq!(point_records[1].rules_count, 2);

        assert_eq!(point_records[3].lines_offset, 6);
        assert_eq!(point_records[3].lines_count, 2);
        assert_eq!(point_records[3].rules_offset, 2);
        assert_eq!(point_records[3].rules_count, 1);

        assert_eq!(point_records[4].rules_count, 0);
        assert_eq!(point_records[5].rules_count, 0);
    }

    #[test]
    fn test_build_point_layout_sorts_points_within_each_cell_by_osm_id() {
        let mut graph = GenerationGraph::new();

        for node in [
            OsmNode {
                id: 30,
                lat: 1.0,
                lon: 1.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 10,
                lat: 1.0,
                lon: 1.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
            OsmNode {
                id: 20,
                lat: 1.0,
                lon: 1.0,
                residential_in_proximity: false,
                nogo_area: false,
            },
        ] {
            graph.insert_node(node);
        }

        graph.insert_way(make_way(10, &[30, 10, 20]));

        let point_layout = RmdfWriter::build_point_layout(&graph).unwrap();

        assert_eq!(
            point_layout
                .ordered_points
                .iter()
                .map(|ordered_point| ordered_point.point.id)
                .collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn test_serialize_line_refs_match_point_slices() {
        let writer = RmdfWriter::new(1.0);
        let graph = writer_test_graph();
        let point_layout = RmdfWriter::build_point_layout(&graph).unwrap();
        let point_records =
            parse_pod_records::<PointRecord>(&writer.serialize_points(&point_layout).unwrap());
        let line_refs = parse_u64_values(&writer.serialize_line_refs(&point_layout).unwrap());

        assert_eq!(line_refs, vec![0, 0, 1, 2, 3, 1, 2, 4, 3, 4]);
        assert_eq!(slice_for_point(&line_refs, &point_records[0]), vec![0]);
        assert_eq!(
            slice_for_point(&line_refs, &point_records[1]),
            vec![0, 1, 2, 3]
        );
        assert_eq!(slice_for_point(&line_refs, &point_records[3]), vec![2, 4]);
    }

    #[test]
    fn test_serialize_rules_writes_rule_records_and_payload_slices() {
        let writer = RmdfWriter::new(1.0);
        let graph = writer_test_graph();
        let point_layout = RmdfWriter::build_point_layout(&graph).unwrap();
        let serialized_rules = writer.serialize_rules(&graph, &point_layout).unwrap();

        assert_eq!(serialized_rules.rule_record_count, 3);

        let rule_record_size = std::mem::size_of::<RuleRecord>();
        let rule_records = parse_pod_records::<RuleRecord>(
            &serialized_rules.bytes
                [..serialized_rules.rule_record_count as usize * rule_record_size],
        );
        let payload = parse_u64_values(
            &serialized_rules.bytes
                [serialized_rules.rule_record_count as usize * rule_record_size..],
        );

        assert_eq!(payload, vec![0, 1, 2, 0, 1, 3, 2, 4]);

        assert_eq!(rule_records[0].from_lines_offset, 0);
        assert_eq!(rule_records[0].from_lines_count, 2);
        assert_eq!(rule_records[0].to_lines_offset, 2);
        assert_eq!(rule_records[0].to_lines_count, 1);
        assert_eq!(
            rule_records[0].rule_type,
            RmdfWriter::serialize_rule_type(&GenerationRestrictionRuleType::NotAllowed)
        );
        assert_eq!(slice_for_rule(&payload, &rule_records[0], true), vec![0, 1]);
        assert_eq!(slice_for_rule(&payload, &rule_records[0], false), vec![2]);

        assert_eq!(rule_records[1].from_lines_offset, 3);
        assert_eq!(rule_records[1].from_lines_count, 2);
        assert_eq!(rule_records[1].to_lines_offset, 5);
        assert_eq!(rule_records[1].to_lines_count, 1);
        assert_eq!(
            rule_records[1].rule_type,
            RmdfWriter::serialize_rule_type(&GenerationRestrictionRuleType::OnlyAllowed)
        );
        assert_eq!(slice_for_rule(&payload, &rule_records[1], true), vec![0, 1]);
        assert_eq!(slice_for_rule(&payload, &rule_records[1], false), vec![3]);

        assert_eq!(rule_records[2].from_lines_offset, 6);
        assert_eq!(rule_records[2].from_lines_count, 1);
        assert_eq!(rule_records[2].to_lines_offset, 7);
        assert_eq!(rule_records[2].to_lines_count, 1);
        assert_eq!(slice_for_rule(&payload, &rule_records[2], true), vec![2]);
        assert_eq!(slice_for_rule(&payload, &rule_records[2], false), vec![4]);
    }

    #[test]
    fn test_write_tile_header_rule_count_tracks_rule_records_only() {
        let writer = RmdfWriter::new(1.0);
        let graph = writer_test_graph();
        let tile_id = TileId { col: 1, row: 2 };
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!("rmdf-writer-phase2-{unique}.rmdf"));

        writer
            .write_tile_from_graph(tile_id, graph, &output_path)
            .unwrap();

        let file_bytes = fs::read(&output_path).unwrap();
        let header: &RmdfHeader = try_from_bytes(&file_bytes[..RmdfHeader::SIZE]).unwrap();
        assert_eq!(header.rule_count, 3);

        let rules_offset = header.section_offsets[section::RULES] as usize;
        let rule_record_bytes = header.rule_count as usize * std::mem::size_of::<RuleRecord>();
        let rules_section = &file_bytes[rules_offset..file_bytes.len() - Sha256::output_size()];
        assert!(rules_section.len() > rule_record_bytes);

        fs::remove_file(output_path).unwrap();
    }

    fn writer_test_graph() -> GenerationGraph {
        let mut graph = GenerationGraph::new();

        for id in 1..=6 {
            graph.insert_node(OsmNode {
                id,
                lat: id as f64,
                lon: id as f64,
                residential_in_proximity: false,
                nogo_area: false,
            });
        }

        graph.insert_way(make_way(10, &[1, 2, 3]));
        graph.insert_way(make_way(20, &[2, 4]));
        graph.insert_way(make_way(30, &[5, 2]));
        graph.insert_way(make_way(40, &[4, 6]));

        graph.insert_relation(make_relation(
            100,
            "no_left_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 20),
            ],
        ));
        graph.insert_relation(make_relation(
            101,
            "only_right_turn",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 10),
                relation_node_member(OsmRelationMemberRole::Via, 2),
                relation_way_member(OsmRelationMemberRole::To, 30),
            ],
        ));
        graph.insert_relation(make_relation(
            102,
            "no_straight_on",
            vec![
                relation_way_member(OsmRelationMemberRole::From, 20),
                relation_node_member(OsmRelationMemberRole::Via, 4),
                relation_way_member(OsmRelationMemberRole::To, 40),
            ],
        ));

        graph
    }

    fn make_way(id: u64, point_ids: &[u64]) -> OsmWay {
        OsmWay {
            id,
            point_ids: point_ids.to_vec(),
            tags: Some(HashMap::from([(
                "highway".to_string(),
                "primary".to_string(),
            )])),
        }
    }

    fn make_relation(id: u64, restriction: &str, members: Vec<OsmRelationMember>) -> OsmRelation {
        OsmRelation {
            id,
            members,
            tags: HashMap::from([
                ("type".to_string(), "restriction".to_string()),
                ("restriction".to_string(), restriction.to_string()),
            ]),
        }
    }

    fn relation_way_member(role: OsmRelationMemberRole, member_ref: u64) -> OsmRelationMember {
        OsmRelationMember {
            member_type: OsmRelationMemberType::Way,
            role,
            member_ref,
        }
    }

    fn relation_node_member(role: OsmRelationMemberRole, member_ref: u64) -> OsmRelationMember {
        OsmRelationMember {
            member_type: OsmRelationMemberType::Node,
            role,
            member_ref,
        }
    }

    fn parse_pod_records<T: Pod>(bytes: &[u8]) -> Vec<T> {
        bytes
            .chunks_exact(std::mem::size_of::<T>())
            .map(pod_read_unaligned)
            .collect()
    }

    fn parse_u64_values(bytes: &[u8]) -> Vec<u64> {
        bytes
            .chunks_exact(std::mem::size_of::<u64>())
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn slice_for_point(line_refs: &[u64], point_record: &PointRecord) -> Vec<u64> {
        let start = point_record.lines_offset as usize;
        let end = start + point_record.lines_count as usize;
        line_refs[start..end].to_vec()
    }

    fn slice_for_rule(payload: &[u64], rule_record: &RuleRecord, from_side: bool) -> Vec<u64> {
        let (start, count) = if from_side {
            (
                rule_record.from_lines_offset as usize,
                rule_record.from_lines_count as usize,
            )
        } else {
            (
                rule_record.to_lines_offset as usize,
                rule_record.to_lines_count as usize,
            )
        };

        payload[start..start + count].to_vec()
    }
}
