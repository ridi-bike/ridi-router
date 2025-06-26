use std::{collections::HashMap, fs::File, time::Instant};

use geo::{Contains, Coord, Intersects, LineString, MultiPolygon, Polygon};
use osmpbfreader::{Node, OsmObj, OsmPbfReader, Relation, Way};
use tracing::{error, info};

#[cfg(feature = "debug-with-postgres")]
use crate::map_data::debug_writer::MapDebugWriter;
use crate::map_data::proximity::AreaGrid;

use super::OsmDataReaderError;

enum Boundary {
    Relation(Relation),
    Way(Way),
}

pub struct PbfAreaReader<'a> {
    nodes: HashMap<i64, Node>,
    ways: HashMap<i64, Way>,
    boundaries: Vec<Boundary>,
    pbf: &'a mut OsmPbfReader<File>,
    area_grid: AreaGrid,
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
            boundaries: Vec::new(),
            pbf,
            area_grid: AreaGrid::new(),
        }
    }
    fn get_line_strings_from_boundary(&self, boundary: &Boundary, role: &str) -> Vec<LineString> {
        let mut boundaries: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut current_boundary: Vec<(f64, f64)> = Vec::new();

        let mut ways_with_points: Vec<WayWithPoints> = Vec::new();

        match boundary {
            Boundary::Relation(relation) => {
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
            }
            Boundary::Way(way) => {
                ways_with_points.push(WayWithPoints {
                    points: way
                        .nodes
                        .iter()
                        .filter_map(|node_id| self.nodes.get(&node_id.0))
                        .map(|node| (node.lat(), node.lon()))
                        .collect(),
                });
            }
        };

        while !ways_with_points.is_empty() {
            if current_boundary.len() == 0 {
                if let Some(way) = ways_with_points.pop() {
                    way.points
                        .iter()
                        .for_each(|p| current_boundary.push((p.0, p.1)));
                }
            } else if let Some(last_point) = current_boundary.last() {
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
                if let Some((idx, next_way)) = next_way {
                    ways_with_points.remove(idx);
                    if let Some(next_way_first_point) = next_way.points.first() {
                        if next_way_first_point == last_point {
                            next_way
                                .points
                                .iter()
                                .for_each(|p| current_boundary.push((p.0, p.1)));
                        } else {
                            next_way
                                .points
                                .iter()
                                .rev()
                                .for_each(|p| current_boundary.push((p.0, p.1)));
                        }
                    }
                } else {
                    if current_boundary.len() > 1 {
                        if let Some(first_point) = current_boundary.first() {
                            if let Some(last_point) = current_boundary.last() {
                                if first_point == last_point {
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
    ) -> MultiPolygon {
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

        MultiPolygon::new(matched_polygons)
    }

    pub fn read<T>(&mut self, selection: &T) -> Result<(), OsmDataReaderError>
    where
        T: Fn(&OsmObj) -> bool,
    {
        info!("Reading boundaries");

        let read_start = Instant::now();

        self.pbf
            .get_objs_and_deps(|el| selection(el))
            .map_err(|error| OsmDataReaderError::PbfFileReadError { error })?
            .into_iter()
            .map(|(_id, element)| {
                if selection(&element) {
                    if let Some(rel) = element.relation() {
                        self.boundaries.push(Boundary::Relation(rel.clone()));
                    } else if let Some(w) = element.way() {
                        self.boundaries.push(Boundary::Way(w.clone()));
                    } else {
                        return Err(OsmDataReaderError::PbfFileError {
                            error: String::from("Expected way or relation"),
                        });
                    }
                } else if let Some(way) = element.way() {
                    self.ways.insert(element.id().inner_id(), way.clone());
                } else if let Some(node) = element.node() {
                    self.nodes.insert(element.id().inner_id(), node.clone());
                }
                Ok(())
            })
            .collect::<Result<(), OsmDataReaderError>>()?;

        let boundaries = self
            .boundaries
            .iter()
            .map(|boundary| {
                let border_outer_points = self.get_line_strings_from_boundary(boundary, "outer");
                let border_inner_points = self.get_line_strings_from_boundary(boundary, "inner");

                Ok(Self::match_holes_to_outer_polygons(
                    &border_outer_points,
                    &border_inner_points,
                ))
            })
            .collect::<Result<Vec<_>, OsmDataReaderError>>()?;

        let read_duration = read_start.elapsed().as_secs();
        info!(duration = read_duration, "Boundary read done");

        let point_grid_started = Instant::now();

        #[cfg(feature = "debug-with-postgres")]
        let mut debug_writer = MapDebugWriter::new();
        boundaries.into_iter().for_each(|multi_polygon| {
            #[cfg(feature = "debug-with-postgres")]
            debug_writer.write_area_residential(&multi_polygon);

            let _adjusted = self.area_grid.insert_multi_polygon(&multi_polygon);

            #[cfg(feature = "debug-with-postgres")]
            debug_writer.write_area_residential_adjusted(&_adjusted);
        });
        #[cfg(feature = "debug-with-postgres")]
        {
            debug_writer.write_line_grid();
            debug_writer.flush();
        }

        let point_grid_duration = point_grid_started.elapsed().as_secs();
        let point_grid_size = self.area_grid.len();
        info!(
            duration = point_grid_duration,
            size = point_grid_size,
            "PointGrid insert done"
        );

        Ok(())
    }

    pub fn get_area_grid(self) -> AreaGrid {
        self.area_grid
    }
}
