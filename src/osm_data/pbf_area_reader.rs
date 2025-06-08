use std::{collections::HashMap, fs::File, io, time::Instant};

use geo::{
    BoundingRect, Contains, Coord, Distance, Haversine, HaversineClosestPoint, LineString,
    MultiPolygon, Polygon,
};
use geo::{CoordsIter, Point as GeoPoint};
use osmpbfreader::{Node, OsmObj, OsmPbfReader, Relation, Way};
use rstar::{Point, PointDistance, RTree, RTreeObject, AABB};
use tracing::{error, info, trace};

#[derive(Debug, thiserror::Error)]
pub enum PbfAreaReaderError {
    #[error("File open error: {error}")]
    PbfFileOpenError { error: io::Error },

    #[error("File read error: {error}")]
    PbfFileReadError { error: osmpbfreader::Error },

    #[error("Name not found for relation {id}")]
    NameNotFound { id: i64 },

    #[error("Level not found for relation {id}")]
    LevelNotFound { id: i64 },
}

#[derive(Debug, PartialEq, Clone)]
struct Area(MultiPolygon);

impl RTreeObject for Area {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        if let Some(bounding_rect) = self.0.bounding_rect() {
            AABB::from_corners(
                [bounding_rect.min().x, bounding_rect.min().y],
                [bounding_rect.max().x, bounding_rect.max().y],
            )
        } else {
            AABB::from_corners([0.0, 0.0], [0.0, 0.0])
        }
    }
}

impl PointDistance for Area {
    fn distance_2(
        &self,
        point: &<Self::Envelope as rstar::Envelope>::Point,
    ) -> <<Self::Envelope as rstar::Envelope>::Point as Point>::Scalar {
        let geo_point = GeoPoint::new(point[0], point[1]);

        match self.0.haversine_closest_point(&geo_point) {
            geo::Closest::Intersection(_) => 0.,
            geo::Closest::SinglePoint(p) => Haversine::distance(p, geo_point).powi(2),
            geo::Closest::Indeterminate => self.0.coords_iter().fold(10000., |min, coords| {
                let dist = Haversine::distance(geo_point, GeoPoint::from(coords));
                if dist < min { dist } else { min }.powi(2)
            }),
        }
    }
}

pub struct PbfAreaReader<'a> {
    nodes: HashMap<i64, Node>,
    ways: HashMap<i64, Way>,
    relations: Vec<Relation>,
    pbf: &'a mut OsmPbfReader<File>,
    pub tree: RTree<Area>,
}

#[derive(Clone, Debug)]
struct WayWithPoints {
    points: Vec<(f64, f64)>,
}

impl<'a> PbfAreaReader<'a> {
    pub fn new(pbf: &'a mut OsmPbfReader<File>) -> Self {
        Self {
            nodes: HashMap::new(),
            ways: HashMap::new(),
            relations: Vec::new(),
            pbf,
            tree: RTree::new(),
        }
    }
    fn get_boundary_from_relation(&self, relation: &Relation, role: &str) -> Vec<LineString> {
        let mut boundaries: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut current_boundary: Vec<(f64, f64)> = Vec::new();

        let mut ways_with_points: Vec<WayWithPoints> = Vec::new();

        for relation_ref in relation
            .refs
            .iter()
            .filter(|relation_ref| relation_ref.role == role)
        {
            let way_id = match relation_ref.member.way() {
                None => {
                    error!(relation_id = ?relation_ref, "Not a way");
                    continue;
                }
                Some(w_id) => w_id,
            };

            let way = match self.ways.get(&way_id.0) {
                None => {
                    continue;
                }
                Some(w) => w,
            };

            ways_with_points.push(WayWithPoints {
                points: way
                    .nodes
                    .iter()
                    .filter_map(|node_id| self.nodes.get(&node_id.0))
                    .map(|node| (node.lat(), node.lon()))
                    .collect(),
            });
        }
        trace!(
            ways_with_points = ?ways_with_points,
            "Prep done"
        );

        while !ways_with_points.is_empty() {
            trace!(
                ways_with_points_len = ways_with_points.len(),
                current_boundary_len = current_boundary.len(),
                "Loop start"
            );
            if current_boundary.len() == 0 {
                if let Some(way) = ways_with_points.pop() {
                    way.points
                        .iter()
                        .for_each(|p| current_boundary.push((p.0, p.1)));
                }
            } else if let Some(last_point) = current_boundary.last() {
                trace!(last_point = ?last_point, "Checking last point");
                let next_way = ways_with_points
                    .iter()
                    .enumerate()
                    .find(|(_i, w)| {
                        if let Some(p) = w.points.last() {
                            if p.0 == last_point.0 && p.1 == last_point.1 {
                                return true;
                            }
                        }
                        if let Some(p) = w.points.first() {
                            if p.0 == last_point.0 && p.1 == last_point.1 {
                                return true;
                            }
                        }
                        false
                    })
                    .map(|w| (w.0, w.1.clone()));
                trace!(next_way = ?next_way, "Next way");
                if let Some((idx, next_way)) = next_way {
                    ways_with_points.remove(idx);
                    if let Some(next_way_first_point) = next_way.points.first() {
                        trace!(next_way_first_point = ?next_way_first_point, "First point match");
                        if next_way_first_point == last_point {
                            next_way
                                .points
                                .iter()
                                .for_each(|p| current_boundary.push((p.0, p.1)));
                        } else {
                            trace!("Last point match");
                            next_way
                                .points
                                .iter()
                                .rev()
                                .for_each(|p| current_boundary.push((p.0, p.1)));
                        }
                    }
                } else {
                    trace!("Should be a full circle");
                    if current_boundary.len() > 1 {
                        if let Some(first_point) = current_boundary.first() {
                            if let Some(last_point) = current_boundary.last() {
                                if first_point == last_point {
                                    trace!("Adding boundary");
                                    boundaries.push(current_boundary);
                                }
                                current_boundary = Vec::new();
                            }
                        }
                    }
                }
            }
        }

        if current_boundary.len() > 1 {
            if let Some(first_point) = current_boundary.first() {
                if let Some(last_point) = current_boundary.last() {
                    if first_point == last_point {
                        boundaries.push(current_boundary);
                    }
                }
            }
        }

        boundaries
            .into_iter()
            .map(|line| {
                LineString::new(
                    line.iter()
                        .map(|(lat, lon)| Coord { x: *lon, y: *lat })
                        .collect(),
                )
            })
            .collect()
    }

    fn match_holes_to_outer_polygons(
        outer_polygons: &[LineString<f64>],
        inner_polygons: &[LineString<f64>],
    ) -> Area {
        let mut matched_polygons = Vec::new();

        for outer in outer_polygons {
            let outer_polygon = Polygon::new(outer.clone(), vec![]);
            let mut matching_holes = Vec::new();

            for inner in inner_polygons {
                if let Some(point) = inner.points().next() {
                    if outer_polygon.contains(&point) {
                        matching_holes.push(inner.clone());
                    }
                }
            }

            let polygon = Polygon::new(outer.clone(), matching_holes);
            matched_polygons.push(polygon);
        }

        Area(MultiPolygon::new(matched_polygons))
    }

    pub fn read<T>(&mut self, selection: T) -> Result<(), PbfAreaReaderError>
    where
        T: FnMut(&OsmObj) -> bool,
    {
        info!("Reading boundaries");

        let read_start = Instant::now();

        self.pbf
            .get_objs_and_deps(selection)
            .map_err(|error| PbfAreaReaderError::PbfFileReadError { error })?
            .into_iter()
            .for_each(|(_id, element)| {
                if element.is_relation()
                    && element.tags().contains("type", "boundary")
                    && element.tags().contains("boundary", "administrative")
                    && element.tags().contains_key("admin_level")
                    && element.tags().contains_key("name")
                {
                    let relation = element.relation().expect("Must be a way");
                    self.relations.push(relation.clone());
                } else if element.is_way() {
                    let way = element.way().expect("Must be a way");
                    self.ways.insert(element.id().inner_id(), way.clone());
                } else if element.is_node() {
                    let node = element.node().expect("Must be a node");
                    self.nodes.insert(element.id().inner_id(), node.clone());
                }
            });

        let boundaries = self
            .relations
            .iter()
            .map(|relation| {
                let border_outer_points = self.get_boundary_from_relation(relation, "outer");
                let border_inner_points = self.get_boundary_from_relation(relation, "inner");

                Ok(Self::match_holes_to_outer_polygons(
                    &border_outer_points,
                    &border_inner_points,
                ))
            })
            .collect::<Result<Vec<_>, PbfAreaReaderError>>()?;

        let read_duration = read_start.elapsed();
        info!(duration = ?read_duration, "Boundary read done");

        info!("Boundary Tree insert started");
        let tree_started = Instant::now();

        boundaries
            .into_iter()
            .for_each(|multi_polygon| self.tree.insert(multi_polygon));

        info!(duration = ?tree_started, "Boundary Tree insert done");

        Ok(())
    }
}
