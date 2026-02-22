use crate::map_data::osm::{OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberType, OsmWay};
use crate::rmdf::format::TileBounds;
use anyhow::{Context, Result};
use geo::{Coord, LineString, MultiPolygon, Polygon};
use osmpbfreader::{OsmObj, OsmPbfReader};
use rstar::{RTree, RTreeObject, AABB};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tracing::{info, warn};

/// Pre-computed bounding box for spatial indexing
#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

impl BoundingBox {
    /// Create an empty bounding box
    pub fn empty() -> Self {
        Self {
            lat_min: f64::MAX,
            lat_max: f64::MIN,
            lon_min: f64::MAX,
            lon_max: f64::MIN,
        }
    }

    /// Create a bounding box from a single point
    pub fn from_point(lat: f64, lon: f64) -> Self {
        Self {
            lat_min: lat,
            lat_max: lat,
            lon_min: lon,
            lon_max: lon,
        }
    }

    /// Check if the bounding box is empty (no points added)
    pub fn is_empty(&self) -> bool {
        self.lat_min > self.lat_max || self.lon_min > self.lon_max
    }

    /// Expand the bounding box to include a point
    pub fn expand_point(&mut self, lat: f64, lon: f64) {
        self.lat_min = self.lat_min.min(lat);
        self.lat_max = self.lat_max.max(lat);
        self.lon_min = self.lon_min.min(lon);
        self.lon_max = self.lon_max.max(lon);
    }

    /// Expand the bounding box to include another bounding box
    pub fn expand_bbox(&mut self, other: &BoundingBox) {
        if !other.is_empty() {
            self.lat_min = self.lat_min.min(other.lat_min);
            self.lat_max = self.lat_max.max(other.lat_max);
            self.lon_min = self.lon_min.min(other.lon_min);
            self.lon_max = self.lon_max.max(other.lon_max);
        }
    }

    /// Convert to RTree AABB for spatial queries
    pub fn to_aabb(&self) -> AABB<[f64; 2]> {
        AABB::from_corners(
            [self.lon_min, self.lat_min],
            [self.lon_max, self.lat_max],
        )
    }

    /// Check if this bounding box intersects with tile bounds
    pub fn intersects_tile_bounds(&self, bounds: &TileBounds) -> bool {
        !(self.lat_max < bounds.lat_min as f64
            || self.lat_min > bounds.lat_max as f64
            || self.lon_max < bounds.lon_min as f64
            || self.lon_min > bounds.lon_max as f64)
    }
}

/// OSM Way with pre-computed bounding box
#[derive(Clone, Debug)]
pub struct WayWithBounds {
    pub way: OsmWay,
    pub bbox: BoundingBox,
}

/// OSM Relation with pre-computed bounding box
#[derive(Clone, Debug)]
pub struct RelationWithBounds {
    pub relation: OsmRelation,
    pub bbox: BoundingBox,
}

/// Spatial index entry for nodes
#[derive(Clone, Debug)]
struct NodeSpatialEntry {
    id: u64,
    point: [f64; 2], // [lon, lat]
}

impl RTreeObject for NodeSpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

/// Spatial index entry for ways
#[derive(Clone, Debug)]
struct WaySpatialEntry {
    id: u64,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for WaySpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Spatial index entry for relations
#[derive(Clone, Debug)]
struct RelationSpatialEntry {
    id: u64,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for RelationSpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Geographic bounds discovered from PBF file
#[derive(Debug, Clone, Copy)]
pub struct PbfBounds {
    pub lat_min: Option<f64>,
    pub lat_max: Option<f64>,
    pub lon_min: Option<f64>,
    pub lon_max: Option<f64>,
}

impl PbfBounds {
    pub fn empty() -> Self {
        Self {
            lat_min: None,
            lat_max: None,
            lon_min: None,
            lon_max: None,
        }
    }

    pub fn update(&mut self, lat: f64, lon: f64) {
        self.lat_min = Some(self.lat_min.map_or(lat, |v| v.min(lat)));
        self.lat_max = Some(self.lat_max.map_or(lat, |v| v.max(lat)));
        self.lon_min = Some(self.lon_min.map_or(lon, |v| v.min(lon)));
        self.lon_max = Some(self.lon_max.map_or(lon, |v| v.max(lon)));
    }

    pub fn is_valid(&self) -> bool {
        self.lat_min.is_some()
            && self.lat_max.is_some()
            && self.lon_min.is_some()
            && self.lon_max.is_some()
    }

    pub fn extract(&self) -> (f64, f64, f64, f64) {
        (
            self.lat_min.unwrap_or(0.0),
            self.lat_max.unwrap_or(0.0),
            self.lon_min.unwrap_or(0.0),
            self.lon_max.unwrap_or(0.0),
        )
    }
}

/// In-memory PBF representation with spatial indexing
pub struct InMemoryPbf {
    // ID-based lookups (for dependency resolution)
    nodes_by_id: HashMap<u64, OsmNode>,
    ways_by_id: HashMap<u64, WayWithBounds>,
    relations_by_id: HashMap<u64, RelationWithBounds>,

    // Spatial indexes (for tile-based queries)
    nodes_spatial: RTree<NodeSpatialEntry>,
    ways_spatial: RTree<WaySpatialEntry>,
    relations_spatial: RTree<RelationSpatialEntry>,


    // Metadata
    pub bounds: PbfBounds,
}

impl Default for InMemoryPbf {
    fn default() -> Self {
        Self {
            nodes_by_id: HashMap::new(),
            ways_by_id: HashMap::new(),
            relations_by_id: HashMap::new(),
            nodes_spatial: RTree::new(),
            ways_spatial: RTree::new(),
            relations_spatial: RTree::new(),
            bounds: PbfBounds::empty(),
        }
    }
}

impl InMemoryPbf {
    /// Load PBF file with pre-computed proximity flags (optimized single-pass approach)
    ///
    /// This method pre-computes residential_in_proximity and nogo_area flags during
    /// the loading phase using a rasterized grid approach with SIMD acceleration.
    /// This eliminates the need for per-tile proximity computation during generation.
    pub fn from_pbf_file_with_flags(path: &Path) -> Result<Self> {
        let start = Instant::now();
        info!("Loading PBF file with pre-computed flags: {:?}", path);

        // Pass 1: Load all nodes
        info!("Pass 1: Loading nodes...");
        let (mut nodes_by_id, nodes_spatial, bounds) = Self::load_nodes(path)?;
        info!(
            "Loaded {} nodes in {:.2}s",
            nodes_by_id.len(),
            start.elapsed().as_secs_f64()
        );

        // Pass 2: Load all ways + compute bounding boxes
        info!("Pass 2: Loading ways...");
        let pass2_start = Instant::now();
        let (ways_by_id, ways_spatial, residential_ways, military_ways) =
            Self::load_ways(path, &nodes_by_id)?;
        info!(
            "Loaded {} ways ({} residential, {} military) in {:.2}s",
            ways_by_id.len(),
            residential_ways.len(),
            military_ways.len(),
            pass2_start.elapsed().as_secs_f64()
        );

        // Pass 3: Load all relations + compute bounding boxes (iterative)
        info!("Pass 3: Loading relations...");
        let pass3_start = Instant::now();
        let (relations_by_id, relations_spatial, residential_relations, military_relations) =
            Self::load_relations(path, &nodes_by_id, &ways_by_id)?;
        info!(
            "Loaded {} relations ({} residential, {} military) in {:.2}s",
            relations_by_id.len(),
            residential_relations.len(),
            military_relations.len(),
            pass3_start.elapsed().as_secs_f64()
        );

        // Step 4: Extract area polygons
        info!("Extracting area polygons...");
        let polygons_start = Instant::now();
        let residential_polygons = Self::extract_area_polygons(
            &residential_ways,
            &residential_relations,
            &ways_by_id,
            &relations_by_id,
            &nodes_by_id,
        );
        let military_polygons = Self::extract_area_polygons(
            &military_ways,
            &military_relations,
            &ways_by_id,
            &relations_by_id,
            &nodes_by_id,
        );
        info!(
            "Extracted {} residential and {} military polygons in {:.2}s",
            residential_polygons.len(),
            military_polygons.len(),
            polygons_start.elapsed().as_secs_f64()
        );

        // Step 5: Compute and apply proximity flags
        info!("Computing proximity flags...");
        let flags_start = Instant::now();
        crate::proximity::compute_proximity_flags(
            &mut nodes_by_id,
            &residential_polygons,
            &military_polygons,
            &bounds,
        );
        info!(
            "Proximity flags computed in {:.2}s",
            flags_start.elapsed().as_secs_f64()
        );

        // Rebuild node spatial index with updated flags (nodes changed in place)
        info!("Rebuilding spatial indexes...");
        let spatial_entries: Vec<NodeSpatialEntry> = nodes_by_id
            .values()
            .map(|node| NodeSpatialEntry {
                id: node.id,
                point: [node.lon, node.lat],
            })
            .collect();
        let nodes_spatial = RTree::bulk_load(spatial_entries);

        info!(
            "PBF loading with flags complete in {:.2}s",
            start.elapsed().as_secs_f64()
        );

        Ok(Self {
            nodes_by_id,
            ways_by_id,
            relations_by_id,
            nodes_spatial,
            ways_spatial,
            relations_spatial,
            bounds,
        })
    }

    /// Load PBF file with pre-computed proximity flags and return the proximity grid.
    ///
    /// This is like `from_pbf_file_with_flags` but also returns the `RasterizedProximityGrid`
    /// for multi-PBF scenarios where grids need to be combined and re-applied in overlap zones.
    ///
    /// # Returns
    ///
    /// A tuple of `(InMemoryPbf, RasterizedProximityGrid)` containing the loaded PBF data
    /// and the computed proximity grid.
    pub fn from_pbf_file_with_grid(
        path: &Path,
    ) -> Result<(Self, crate::proximity::RasterizedProximityGrid)> {
        let start = Instant::now();
        info!("Loading PBF file with pre-computed flags and grid: {:?}", path);

        // Pass 1: Load all nodes
        info!("Pass 1: Loading nodes...");
        let (mut nodes_by_id, nodes_spatial, bounds) = Self::load_nodes(path)?;
        info!(
            "Loaded {} nodes in {:.2}s",
            nodes_by_id.len(),
            start.elapsed().as_secs_f64()
        );

        // Pass 2: Load all ways + compute bounding boxes
        info!("Pass 2: Loading ways...");
        let pass2_start = Instant::now();
        let (ways_by_id, ways_spatial, residential_ways, military_ways) =
            Self::load_ways(path, &nodes_by_id)?;
        info!(
            "Loaded {} ways ({} residential, {} military) in {:.2}s",
            ways_by_id.len(),
            residential_ways.len(),
            military_ways.len(),
            pass2_start.elapsed().as_secs_f64()
        );

        // Pass 3: Load all relations + compute bounding boxes (iterative)
        info!("Pass 3: Loading relations...");
        let pass3_start = Instant::now();
        let (relations_by_id, relations_spatial, residential_relations, military_relations) =
            Self::load_relations(path, &nodes_by_id, &ways_by_id)?;
        info!(
            "Loaded {} relations ({} residential, {} military) in {:.2}s",
            relations_by_id.len(),
            residential_relations.len(),
            military_relations.len(),
            pass3_start.elapsed().as_secs_f64()
        );

        // Step 4: Extract area polygons
        info!("Extracting area polygons...");
        let polygons_start = Instant::now();
        let residential_polygons = Self::extract_area_polygons(
            &residential_ways,
            &residential_relations,
            &ways_by_id,
            &relations_by_id,
            &nodes_by_id,
        );
        let military_polygons = Self::extract_area_polygons(
            &military_ways,
            &military_relations,
            &ways_by_id,
            &relations_by_id,
            &nodes_by_id,
        );
        info!(
            "Extracted {} residential and {} military polygons in {:.2}s",
            residential_polygons.len(),
            military_polygons.len(),
            polygons_start.elapsed().as_secs_f64()
        );

        // Step 5: Compute and apply proximity flags, returning the grid
        info!("Computing proximity flags...");
        let flags_start = Instant::now();
        let grid = crate::proximity::compute_proximity_flags_with_grid(
            &mut nodes_by_id,
            &residential_polygons,
            &military_polygons,
            &bounds,
        );
        info!(
            "Proximity flags computed in {:.2}s",
            flags_start.elapsed().as_secs_f64()
        );

        // Rebuild node spatial index with updated flags (nodes changed in place)
        info!("Rebuilding spatial indexes...");
        let spatial_entries: Vec<NodeSpatialEntry> = nodes_by_id
            .values()
            .map(|node| NodeSpatialEntry {
                id: node.id,
                point: [node.lon, node.lat],
            })
            .collect();
        let nodes_spatial = RTree::bulk_load(spatial_entries);

        info!(
            "PBF loading with flags and grid complete in {:.2}s",
            start.elapsed().as_secs_f64()
        );

        Ok((
            Self {
                nodes_by_id,
                ways_by_id,
                relations_by_id,
                nodes_spatial,
                ways_spatial,
                relations_spatial,
                bounds,
            },
            grid,
        ))
    }

    /// Extract MultiPolygon geometries from area ways and relations
    fn extract_area_polygons(
        way_ids: &[u64],
        relation_ids: &[u64],
        ways_by_id: &HashMap<u64, WayWithBounds>,
        relations_by_id: &HashMap<u64, RelationWithBounds>,
        nodes_by_id: &HashMap<u64, OsmNode>,
    ) -> Vec<MultiPolygon<f64>> {
        let mut polygons = Vec::new();

        // Extract polygons from closed ways
        for way_id in way_ids {
            if let Some(way_with_bounds) = ways_by_id.get(way_id) {
                let way = &way_with_bounds.way;

                // Check if way is closed (first node == last node)
                if way.point_ids.is_empty() || way.point_ids.first() != way.point_ids.last() {
                    continue;
                }

                // Convert nodes to coordinates
                let coords: Vec<Coord<f64>> = way
                    .point_ids
                    .iter()
                    .filter_map(|node_id| nodes_by_id.get(node_id))
                    .map(|node| Coord {
                        x: node.lon,
                        y: node.lat,
                    })
                    .collect();

                if coords.len() >= 4 {
                    // Need at least 4 points for a valid polygon (3 + closing point)
                    let line_string = LineString::new(coords);
                    let polygon = Polygon::new(line_string, vec![]);
                    polygons.push(MultiPolygon::new(vec![polygon]));
                }
            }
        }

        // Extract polygons from multipolygon relations
        for relation_id in relation_ids {
            if let Some(rel_with_bounds) = relations_by_id.get(relation_id) {
                let relation = &rel_with_bounds.relation;

                // Check if this is a multipolygon
                if relation.tags.get("type").map(|v| v.as_str()) != Some("multipolygon") {
                    // For non-multipolygon relations, check if member ways form polygons
                    for member in &relation.members {
                        if member.member_type == OsmRelationMemberType::Way {
                            if let Some(way_with_bounds) = ways_by_id.get(&member.member_ref) {
                                let way = &way_with_bounds.way;
                                if way.point_ids.first() == way.point_ids.last()
                                    && way.point_ids.len() >= 4
                                {
                                    let coords: Vec<Coord<f64>> = way
                                        .point_ids
                                        .iter()
                                        .filter_map(|node_id| nodes_by_id.get(node_id))
                                        .map(|node| Coord {
                                            x: node.lon,
                                            y: node.lat,
                                        })
                                        .collect();

                                    if coords.len() >= 4 {
                                        let line_string = LineString::new(coords);
                                        let polygon = Polygon::new(line_string, vec![]);
                                        polygons.push(MultiPolygon::new(vec![polygon]));
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }

                // Collect outer and inner ways
                let mut outer_rings: Vec<LineString<f64>> = Vec::new();
                let mut inner_rings: Vec<LineString<f64>> = Vec::new();

                for member in &relation.members {
                    if member.member_type != OsmRelationMemberType::Way {
                        continue;
                    }

                    if let Some(way_with_bounds) = ways_by_id.get(&member.member_ref) {
                        let way = &way_with_bounds.way;
                        let coords: Vec<Coord<f64>> = way
                            .point_ids
                            .iter()
                            .filter_map(|node_id| nodes_by_id.get(node_id))
                            .map(|node| Coord {
                                x: node.lon,
                                y: node.lat,
                            })
                            .collect();

                        if coords.len() < 2 {
                            continue;
                        }

                        let line_string = LineString::new(coords);

                        // Check role (outer or inner)
                        let role_str = match &member.role {
                            crate::map_data::osm::OsmRelationMemberRole::Other(s) => s.as_str(),
                            _ => "",
                        };

                        if role_str == "outer" || role_str.is_empty() {
                            outer_rings.push(line_string);
                        } else if role_str == "inner" {
                            inner_rings.push(line_string);
                        }
                    }
                }

                // Build polygons from outer rings with matching inner rings
                for outer_ring in outer_rings {
                    // Close the ring if necessary
                    let mut outer_coords = outer_ring.into_inner();
                    if !outer_coords.is_empty() && outer_coords.first() != outer_coords.last() {
                        outer_coords.push(outer_coords[0]);
                    }

                    if outer_coords.len() < 4 {
                        continue;
                    }

                    let outer_ls = LineString::new(outer_coords);
                    let polygon = Polygon::new(outer_ls, vec![]);

                    // For simplicity, we add inner rings to the first polygon that contains them
                    // A more sophisticated approach would match them properly
                    let mut polygon_with_holes = polygon;
                    for inner_ring in &inner_rings {
                        let mut inner_coords = inner_ring.clone().into_inner();
                        if !inner_coords.is_empty() && inner_coords.first() != inner_coords.last() {
                            inner_coords.push(inner_coords[0]);
                        }
                        if inner_coords.len() >= 4 {
                            // Check if first point of inner is within outer
                            // This is a simplified check - proper containment would be better
                            polygon_with_holes = Polygon::new(
                                polygon_with_holes.exterior().clone(),
                                polygon_with_holes
                                    .interiors()
                                    .iter()
                                    .cloned()
                                    .chain(std::iter::once(LineString::new(inner_coords)))
                                    .collect(),
                            );
                        }
                    }

                    polygons.push(MultiPolygon::new(vec![polygon_with_holes]));
                }
            }
        }

        polygons
    }

    /// Pass 1: Load all nodes from PBF
    fn load_nodes(
        path: &Path,
    ) -> Result<(HashMap<u64, OsmNode>, RTree<NodeSpatialEntry>, PbfBounds)> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open PBF file: {:?}", path))?;
        let mut pbf = OsmPbfReader::new(file);

        let mut bounds = PbfBounds::empty();

        // Collect all nodes in parallel
        let nodes: Vec<_> = pbf
            .par_iter()
            .filter_map(|obj_result| {
                if let Ok(obj) = obj_result {
                    if let OsmObj::Node(node) = obj {
                        let osm_node = OsmNode {
                            id: node.id.0 as u64,
                            lat: node.lat(),
                            lon: node.lon(),
                            residential_in_proximity: false,
                            nogo_area: false,
                        };
                        return Some(osm_node);
                    }
                }
                None
            })
            .collect();

        // Build HashMap and spatial index
        let mut nodes_by_id = HashMap::with_capacity(nodes.len());
        let mut spatial_entries = Vec::with_capacity(nodes.len());

        for node in nodes {
            bounds.update(node.lat, node.lon);

            spatial_entries.push(NodeSpatialEntry {
                id: node.id,
                point: [node.lon, node.lat],
            });

            nodes_by_id.insert(node.id, node);
        }

        let nodes_spatial = RTree::bulk_load(spatial_entries);

        Ok((nodes_by_id, nodes_spatial, bounds))
    }

    /// Pass 2: Load all ways and compute bounding boxes
    fn load_ways(
        path: &Path,
        nodes_by_id: &HashMap<u64, OsmNode>,
    ) -> Result<(
        HashMap<u64, WayWithBounds>,
        RTree<WaySpatialEntry>,
        Vec<u64>,
        Vec<u64>,
    )> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open PBF file: {:?}", path))?;
        let mut pbf = OsmPbfReader::new(file);

        // Collect all ways with highway or area tags
        let ways: Vec<_> = pbf
            .par_iter()
            .filter_map(|obj_result| {
                if let Ok(obj) = obj_result {
                    if let OsmObj::Way(way) = obj {
                        let tags = way.tags.clone();

                        // Check if this way has highway or area tags we care about
                        let has_highway = tags.contains_key("highway");
                        let is_residential = tags.get("landuse").map(|v| v.as_str()) == Some("residential");
                        let is_military = tags.get("landuse").map(|v| v.as_str()) == Some("military");

                        if has_highway || is_residential || is_military {
                            let tags_map: HashMap<String, String> = tags
                                .iter()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect();

                            let osm_way = OsmWay {
                                id: way.id.0 as u64,
                                point_ids: way.nodes.iter().map(|n| n.0 as u64).collect(),
                                tags: if tags_map.is_empty() { None } else { Some(tags_map) },
                            };
                            return Some((osm_way, is_residential, is_military));
                        }
                    }
                }
                None
            })
            .collect();

        // Compute bounding boxes and build structures
        let mut ways_by_id = HashMap::with_capacity(ways.len());
        let mut spatial_entries = Vec::with_capacity(ways.len());
        let mut residential_ways = Vec::new();
        let mut military_ways = Vec::new();

        for (way, is_residential, is_military) in ways {
            // Compute bounding box from nodes
            let mut bbox = BoundingBox::empty();
            let mut missing_nodes = 0;

            for node_id in &way.point_ids {
                if let Some(node) = nodes_by_id.get(node_id) {
                    bbox.expand_point(node.lat, node.lon);
                } else {
                    missing_nodes += 1;
                }
            }

            // Skip ways where all nodes are missing
            if bbox.is_empty() {
                if missing_nodes > 0 {
                    warn!(
                        "Way {} has all nodes missing ({} nodes)",
                        way.id, missing_nodes
                    );
                }
                continue;
            }

            if missing_nodes > 0 {
                warn!(
                    "Way {} is missing {} out of {} nodes",
                    way.id,
                    missing_nodes,
                    way.point_ids.len()
                );
            }

            let way_id = way.id;

            // Track area types
            if is_residential {
                residential_ways.push(way_id);
            }
            if is_military {
                military_ways.push(way_id);
            }

            // Add to spatial index
            spatial_entries.push(WaySpatialEntry {
                id: way_id,
                envelope: bbox.to_aabb(),
            });

            // Store in HashMap
            ways_by_id.insert(
                way_id,
                WayWithBounds {
                    way,
                    bbox,
                },
            );
        }

        let ways_spatial = RTree::bulk_load(spatial_entries);

        Ok((ways_by_id, ways_spatial, residential_ways, military_ways))
    }

    /// Pass 3: Load all relations and compute bounding boxes (iterative)
    fn load_relations(
        path: &Path,
        nodes_by_id: &HashMap<u64, OsmNode>,
        ways_by_id: &HashMap<u64, WayWithBounds>,
    ) -> Result<(
        HashMap<u64, RelationWithBounds>,
        RTree<RelationSpatialEntry>,
        Vec<u64>,
        Vec<u64>,
    )> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open PBF file: {:?}", path))?;
        let mut pbf = OsmPbfReader::new(file);

        // Collect all relations
        let relations: Vec<_> = pbf
            .par_iter()
            .filter_map(|obj_result| {
                if let Ok(obj) = obj_result {
                    if let OsmObj::Relation(relation) = obj {
                        let tags = relation.tags.clone();

                        // Check if this is a multipolygon or area relation
                        let is_multipolygon = tags.get("type").map(|v| v.as_str()) == Some("multipolygon");
                        let is_residential = tags.get("landuse").map(|v| v.as_str()) == Some("residential");
                        let is_military = tags.get("landuse").map(|v| v.as_str()) == Some("military");

                        if is_multipolygon || is_residential || is_military {
                            let members: Vec<OsmRelationMember> = relation
                                .refs
                                .iter()
                                .map(|r| {
                                    let member_type = match r.member {
                                        osmpbfreader::OsmId::Node(_) => OsmRelationMemberType::Node,
                                        osmpbfreader::OsmId::Way(_) => OsmRelationMemberType::Way,
                                        osmpbfreader::OsmId::Relation(_) => OsmRelationMemberType::Relation,
                                    };

                                    let member_ref = match r.member {
                                        osmpbfreader::OsmId::Node(id) => id.0 as u64,
                                        osmpbfreader::OsmId::Way(id) => id.0 as u64,
                                        osmpbfreader::OsmId::Relation(id) => id.0 as u64,
                                    };

                                    OsmRelationMember {
                                        member_type,
                                        role: crate::map_data::osm::OsmRelationMemberRole::Other(r.role.to_string()),
                                        member_ref,
                                    }
                                })
                                .collect();

                            let tags_map: HashMap<String, String> = tags
                                .iter()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect();

                            let osm_relation = OsmRelation {
                                id: relation.id.0 as u64,
                                members,
                                tags: tags_map,
                            };

                            return Some((osm_relation, is_residential, is_military));
                        }
                    }
                }
                None
            })
            .collect();

        info!("Collected {} relations, computing bounding boxes...", relations.len());

        // Compute bounding boxes iteratively (handles nested relations)
        let (relations_with_bounds, residential_relations, military_relations) =
            Self::compute_relation_bboxes_iterative(relations, nodes_by_id, ways_by_id);

        // Build HashMap and spatial index
        let mut relations_by_id = HashMap::with_capacity(relations_with_bounds.len());
        let mut spatial_entries = Vec::with_capacity(relations_with_bounds.len());

        for rel_with_bounds in relations_with_bounds {
            if !rel_with_bounds.bbox.is_empty() {
                spatial_entries.push(RelationSpatialEntry {
                    id: rel_with_bounds.relation.id,
                    envelope: rel_with_bounds.bbox.to_aabb(),
                });
            }

            relations_by_id.insert(rel_with_bounds.relation.id, rel_with_bounds);
        }

        let relations_spatial = RTree::bulk_load(spatial_entries);

        Ok((
            relations_by_id,
            relations_spatial,
            residential_relations,
            military_relations,
        ))
    }

    /// Iteratively compute relation bounding boxes (handles nested relations)
    fn compute_relation_bboxes_iterative(
        relations: Vec<(OsmRelation, bool, bool)>,
        nodes_by_id: &HashMap<u64, OsmNode>,
        ways_by_id: &HashMap<u64, WayWithBounds>,
    ) -> (Vec<RelationWithBounds>, Vec<u64>, Vec<u64>) {
        let mut pending: HashMap<u64, (OsmRelation, bool, bool)> = HashMap::new();
        let mut resolved: HashMap<u64, RelationWithBounds> = HashMap::new();
        let mut residential_relations = Vec::new();
        let mut military_relations = Vec::new();

        // Initialize pending map
        for (relation, is_residential, is_military) in relations {
            pending.insert(relation.id, (relation, is_residential, is_military));
        }

        // Iterate up to 10 times to handle nested relations
        for iteration in 0..10 {
            let mut newly_resolved = Vec::new();

            for (rel_id, (relation, is_residential, is_military)) in &pending {
                if let Some(bbox) = Self::try_compute_relation_bbox(
                    relation,
                    nodes_by_id,
                    ways_by_id,
                    &resolved,
                ) {
                    newly_resolved.push((
                        *rel_id,
                        RelationWithBounds {
                            relation: relation.clone(),
                            bbox,
                        },
                        *is_residential,
                        *is_military,
                    ));
                }
            }

            if newly_resolved.is_empty() {
                if !pending.is_empty() {
                    warn!(
                        "Could not resolve {} relations after {} iterations (circular dependencies or missing members)",
                        pending.len(),
                        iteration + 1
                    );
                }
                break;
            }

            info!(
                "Iteration {}: Resolved {} relations ({} remaining)",
                iteration + 1,
                newly_resolved.len(),
                pending.len() - newly_resolved.len()
            );

            for (rel_id, rel_with_bounds, is_residential, is_military) in newly_resolved {
                pending.remove(&rel_id);

                if is_residential {
                    residential_relations.push(rel_id);
                }
                if is_military {
                    military_relations.push(rel_id);
                }

                resolved.insert(rel_id, rel_with_bounds);
            }
        }

        let relations_with_bounds: Vec<_> = resolved.into_values().collect();

        (relations_with_bounds, residential_relations, military_relations)
    }

    /// Try to compute a relation's bounding box from its members
    fn try_compute_relation_bbox(
        relation: &OsmRelation,
        nodes_by_id: &HashMap<u64, OsmNode>,
        ways_by_id: &HashMap<u64, WayWithBounds>,
        resolved_relations: &HashMap<u64, RelationWithBounds>,
    ) -> Option<BoundingBox> {
        let mut bbox = BoundingBox::empty();
        let mut has_any_members = false;

        for member in &relation.members {
            match member.member_type {
                OsmRelationMemberType::Node => {
                    if let Some(node) = nodes_by_id.get(&member.member_ref) {
                        bbox.expand_point(node.lat, node.lon);
                        has_any_members = true;
                    }
                }
                OsmRelationMemberType::Way => {
                    if let Some(way_with_bounds) = ways_by_id.get(&member.member_ref) {
                        bbox.expand_bbox(&way_with_bounds.bbox);
                        has_any_members = true;
                    }
                }
                OsmRelationMemberType::Relation => {
                    if let Some(rel_with_bounds) = resolved_relations.get(&member.member_ref) {
                        bbox.expand_bbox(&rel_with_bounds.bbox);
                        has_any_members = true;
                    } else {
                        // Relation member not yet resolved, can't compute bbox yet
                        return None;
                    }
                }
            }
        }

        if has_any_members {
            Some(bbox)
        } else {
            // No members found, return empty bbox
            Some(BoundingBox::empty())
        }
    }

    /// Query all nodes within tile bounds
    pub fn query_nodes_in_bounds(&self, bounds: &TileBounds) -> Vec<&OsmNode> {
        let envelope = AABB::from_corners(
            [bounds.lon_min as f64, bounds.lat_min as f64],
            [bounds.lon_max as f64, bounds.lat_max as f64],
        );

        self.nodes_spatial
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|entry| self.nodes_by_id.get(&entry.id))
            .collect()
    }

    /// Query all ways intersecting tile bounds
    pub fn query_ways_in_bounds(&self, bounds: &TileBounds) -> Vec<&WayWithBounds> {
        let envelope = AABB::from_corners(
            [bounds.lon_min as f64, bounds.lat_min as f64],
            [bounds.lon_max as f64, bounds.lat_max as f64],
        );

        self.ways_spatial
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|entry| self.ways_by_id.get(&entry.id))
            .collect()
    }

    /// Query all relations intersecting tile bounds
    pub fn query_relations_in_bounds(&self, bounds: &TileBounds) -> Vec<&RelationWithBounds> {
        let envelope = AABB::from_corners(
            [bounds.lon_min as f64, bounds.lat_min as f64],
            [bounds.lon_max as f64, bounds.lat_max as f64],
        );

        self.relations_spatial
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|entry| self.relations_by_id.get(&entry.id))
            .collect()
    }

}
