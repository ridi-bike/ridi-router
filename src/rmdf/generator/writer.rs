use anyhow::{Context, Result};
use bytemuck::bytes_of;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Write, Seek, SeekFrom};
use std::path::Path;
use tracing::debug;

use crate::map_data::GenerationGraph;
use crate::rmdf::format::*;

use super::intermediate::IntermediateTile;

pub struct RmdfWriter {
    tile_size_degrees: f32,
}

impl RmdfWriter {
    pub fn new(tile_size_degrees: f32) -> Self {
        Self { tile_size_degrees }
    }

    /// Write RMDF file for a single tile
    pub fn write_tile(&self, intermediate: &IntermediateTile, output_path: &Path) -> Result<()> {
        debug!("Writing RMDF file: {:?}", output_path);

        // Step 1: Build GenerationGraph from intermediate data
        let graph = self.build_graph(intermediate)
            .context("Failed to build GenerationGraph")?;

        // Step 2: Build spatial index
        let spatial_index = self.build_spatial_index(&graph)
            .context("Failed to build spatial index")?;

        // Step 3: Prepare all sections
        let points = self.serialize_points(&graph)
            .context("Failed to serialize points")?;
        let lines = self.serialize_lines(&graph)
            .context("Failed to serialize lines")?;
        let line_refs = self.serialize_line_refs(&graph)
            .context("Failed to serialize line refs")?;
        let (tag_values, tag_strings) = self.serialize_tag_values(&graph)
            .context("Failed to serialize tag values")?;
        let tag_sets = self.serialize_tag_sets(&graph)
            .context("Failed to serialize tag sets")?;
        let rules = self.serialize_rules(&graph)
            .context("Failed to serialize rules")?;

        // Step 4: Calculate section offsets
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
        offset += rules.len();

        // Step 5: Build header
        let bounds = self.compute_tile_bounds(intermediate.tile_id);
        let header = RmdfHeader {
            magic: RmdfHeader::MAGIC,
            version: RmdfHeader::VERSION,
            tile_bounds: bounds,
            point_count: graph.get_points().len() as u64,
            line_count: graph.get_lines().len() as u64,
            spatial_grid_cell_count: (spatial_index.len() / std::mem::size_of::<GridCellEntry>()) as u32,
            tag_value_count: (tag_values.len() / std::mem::size_of::<StringEntry>()) as u32,
            tag_set_count: (tag_sets.len() / std::mem::size_of::<TagSetRecord>()) as u32,
            rule_count: (rules.len() / std::mem::size_of::<RuleRecord>()) as u32,
            section_offsets,
        };

        // Step 6: Write all data to file
        let mut file = File::create(output_path)
            .context("Failed to create output file")?;

        file.write_all(bytes_of(&header))
            .context("Failed to write header")?;
        file.write_all(&spatial_index)
            .context("Failed to write spatial index")?;
        file.write_all(&points)
            .context("Failed to write points")?;
        file.write_all(&lines)
            .context("Failed to write lines")?;
        file.write_all(&line_refs)
            .context("Failed to write line refs")?;
        file.write_all(&tag_values)
            .context("Failed to write tag values")?;
        file.write_all(&tag_strings)
            .context("Failed to write tag strings")?;
        file.write_all(&tag_sets)
            .context("Failed to write tag sets")?;
        file.write_all(&rules)
            .context("Failed to write rules")?;

        // Step 7: Compute and append checksum
        file.seek(SeekFrom::Start(0))
            .context("Failed to seek to start for checksum")?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)
            .context("Failed to copy file for checksum")?;
        let checksum = hasher.finalize();

        file.write_all(&checksum)
            .context("Failed to write checksum")?;

        let file_size = file.metadata()
            .context("Failed to get file metadata")?
            .len();
        debug!("RMDF file written: {} bytes", file_size);

        Ok(())
    }

    fn build_graph(&self, intermediate: &IntermediateTile) -> Result<GenerationGraph> {
        let mut graph = GenerationGraph::new();

        // Insert all nodes
        for node in intermediate.nodes.values() {
            graph.insert_node(node.clone());
        }

        // Insert all ways
        for way in &intermediate.ways {
            graph.insert_way(way.clone())
                .context("Failed to insert way")?;
        }

        // Insert all relations
        for relation in &intermediate.relations {
            graph.insert_relation(relation.clone())
                .context("Failed to insert relation")?;
        }

        // Generate spatial index
        graph.generate_point_hashes();

        Ok(graph)
    }

    fn build_spatial_index(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        // Group points by grid cell
        let mut cells: HashMap<u32, Vec<usize>> = HashMap::new();

        for (idx, point) in graph.get_points().iter().enumerate() {
            if point.lines.is_empty() {
                continue; // Skip disconnected points
            }

            let cell_id = GridCellEntry::encode_cell_id(point.lat, point.lon, 100);
            cells.entry(cell_id).or_default().push(idx);
        }

        // Sort cells by cell_id for binary search
        let mut sorted_cells: Vec<_> = cells.into_iter().collect();
        sorted_cells.sort_by_key(|(cell_id, _)| *cell_id);

        // Build GridCellEntry array
        let mut grid_entries = Vec::new();
        let mut points_offset = 0u64;

        for (cell_id, point_indices) in sorted_cells {
            let entry = GridCellEntry {
                cell_id,
                _padding1: 0,
                points_offset,
                points_count: point_indices.len() as u32,
                _padding2: 0,
            };

            grid_entries.push(entry);
            points_offset += point_indices.len() as u64;
        }

        // Serialize to bytes
        let bytes: Vec<u8> = grid_entries.iter()
            .flat_map(|entry| bytes_of(entry).to_vec())
            .collect();

        Ok(bytes)
    }

    fn serialize_points(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        // CRITICAL FIX: Group points by grid cell and sort by cell_id
        // This ensures point indices match the spatial index offsets
        let mut cells: HashMap<u32, Vec<&crate::map_data::point::MapDataPoint>> = HashMap::new();

        for point in graph.get_points() {
            if point.lines.is_empty() {
                continue; // Skip disconnected points (same as build_spatial_index)
            }

            let cell_id = GridCellEntry::encode_cell_id(point.lat, point.lon, 100);
            cells.entry(cell_id).or_default().push(point);
        }

        // Sort cells by cell_id (must match build_spatial_index ordering)
        let mut sorted_cells: Vec<_> = cells.into_iter().collect();
        sorted_cells.sort_by_key(|(cell_id, _)| *cell_id);

        // Serialize points in sorted cell order
        for (_, points_in_cell) in sorted_cells {
            for point in points_in_cell {
                let record = PointRecord {
                    osm_id: point.id,
                    lat: point.lat,
                    lon: point.lon,
                    lines_offset: 0,  // TODO: Calculate from line refs
                    lines_count: point.lines.len() as u32,
                    _padding1: 0,
                    rules_offset: 0,  // TODO: Calculate from rules
                    rules_count: point.rules.len() as u32,
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
        }

        Ok(bytes)
    }

    fn serialize_lines(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        for line in graph.get_lines() {
            let point_a = line.points.0.get();
            let point_b = line.points.1.get();

            let record = LineRecord {
                point_a_osm_id: point_a.id,
                point_a_lat: point_a.lat,
                point_a_lon: point_a.lon,
                point_b_osm_id: point_b.id,
                point_b_lat: point_b.lat,
                point_b_lon: point_b.lon,
                direction: match line.direction {
                    crate::map_data::line::LineDirection::BothWays => 0,
                    crate::map_data::line::LineDirection::OneWay => 1,
                    crate::map_data::line::LineDirection::Roundabout => 2,
                },
                _padding1: 0,
                _padding2: 0,
                tag_set_index: line.tags.tag_set_idx,
            };

            bytes.extend_from_slice(bytes_of(&record));
        }

        Ok(bytes)
    }

    fn serialize_line_refs(&self, graph: &GenerationGraph) -> Result<Vec<u8>> {
        // CRITICAL FIX: Must use identical sorting as serialize_points()
        // Otherwise line_refs indices won't match point indices in the file
        let mut line_refs = Vec::new();

        let mut cells: HashMap<u32, Vec<&crate::map_data::point::MapDataPoint>> = HashMap::new();

        for point in graph.get_points() {
            if point.lines.is_empty() {
                continue; // Skip disconnected points (same as serialize_points)
            }

            let cell_id = GridCellEntry::encode_cell_id(point.lat, point.lon, 100);
            cells.entry(cell_id).or_default().push(point);
        }

        // Sort cells by cell_id (must match serialize_points ordering)
        let mut sorted_cells: Vec<_> = cells.into_iter().collect();
        sorted_cells.sort_by_key(|(cell_id, _)| *cell_id);

        // Flatten line refs in same sorted order as points
        for (_, points_in_cell) in sorted_cells {
            for point in points_in_cell {
                for line_ref in &point.lines {
                    // For now, use line index as reference
                    // TODO: Encode way ID + segment index into u64 when available
                    line_refs.push(line_ref.get_element_id());
                }
            }
        }

        let bytes: Vec<u8> = line_refs.iter()
            .flat_map(|&id| id.to_le_bytes())
            .collect();

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
                name_idx: tag_set.name.tag_value_idx.wrapping_sub(1),
                hw_ref_idx: tag_set.hw_ref.tag_value_idx.wrapping_sub(1),
                highway_idx: tag_set.highway.tag_value_idx.wrapping_sub(1),
                surface_idx: tag_set.surface.tag_value_idx.wrapping_sub(1),
                smoothness_idx: tag_set.smoothness.tag_value_idx.wrapping_sub(1),
            };

            bytes.extend_from_slice(bytes_of(&record));
        }

        Ok(bytes)
    }

    fn serialize_rules(&self, _graph: &GenerationGraph) -> Result<Vec<u8>> {
        // Rules serialization - similar to line refs
        // TODO: Implement based on MapDataRule structure
        Ok(Vec::new())
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
