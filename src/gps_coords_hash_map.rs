use std::{collections::BTreeMap, rc::Rc, u128, u64};

use crate::gps_hash::{get_gps_coords_hash, HashOffset};

#[derive(Clone)]
pub struct GpsCoordsHashMapPoint {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
}

type GpsPointMap = BTreeMap<u64, Rc<GpsCoordsHashMapPoint>>;

pub struct GpsCoordsHashMap {
    points: Vec<Rc<GpsCoordsHashMapPoint>>,
    offset_none: GpsPointMap,
    offset_lat: GpsPointMap,
    offset_lon: GpsPointMap,
    offset_lat_lon: GpsPointMap,
}

impl GpsCoordsHashMap {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            offset_none: BTreeMap::new(),
            offset_lat: BTreeMap::new(),
            offset_lon: BTreeMap::new(),
            offset_lat_lon: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, value: GpsCoordsHashMapPoint) -> () {
        let lat = value.lat.clone();
        let lon = value.lon.clone();
        let point = Rc::new(value);
        self.offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::None),
            point.clone(),
        );
        self.offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::Lat),
            point.clone(),
        );
        self.offset_none.insert(
            get_gps_coords_hash(lat.clone(), lon.clone(), HashOffset::Lon),
            point.clone(),
        );
        self.offset_none.insert(
            get_gps_coords_hash(lat, lon, HashOffset::LatLon),
            point.clone(),
        );
        self.points.push(point);
    }

    pub fn get_closest(&self, lat: f64, lon: f64) -> Option<GpsCoordsHashMapPoint> {
        let search_hash = get_gps_coords_hash(lat, lon, HashOffset::None);

        eprintln!("search hash {}", search_hash);
        eprintln!("coords count {}", self.offset_none.len());

        let mut grid_points: Vec<Rc<GpsCoordsHashMapPoint>> = Vec::new();

        let orientation = search_hash << 62 >> 62;

        eprintln!("orientation {}", orientation);

        for level in 0..=32 {
            let shift_width = 2 * level;
            let from = search_hash >> shift_width << shift_width;
            let to = from
                | if shift_width > 0 {
                    u64::max_value() >> (64 - shift_width)
                } else {
                    search_hash
                };

            // let mask = u64::max_value() ^ ((2 * level) - 1);
            eprintln!("level {} shift_width {}", level, shift_width);
            // let square_size = if level == 0 { 0 } else { 2_u64.pow(level) };
            // let from = if search_hash >= square_size {
            //     search_hash - square_size
            // } else {
            //     0
            // };
            // let to = search_hash;
            // eprintln!("square_size {}", square_size);
            eprintln!("range {:b}..{:b}", from, to);

            let offset_none_points = self.offset_none.range(from..=to);
            let offset_lat_points = self.offset_lat.range(from..=to);
            let offset_lon_points = self.offset_lon.range(from..=to);
            let offset_lat_lon_points = self.offset_lat_lon.range(from..=to);
            let points: [Vec<Rc<GpsCoordsHashMapPoint>>; 4] = [
                offset_none_points.map(|(_, point)| point.clone()).collect(),
                offset_lat_points.map(|(_, point)| point.clone()).collect(),
                offset_lon_points.map(|(_, point)| point.clone()).collect(),
                offset_lat_lon_points
                    .map(|(_, point)| point.clone())
                    .collect(),
            ];

            let points = points.concat();
            if !points.is_empty() || (from == 0 && to == u64::max_value()) {
                grid_points = points;
                break;
            }
        }

        let mut points_with_dist: Vec<(u32, GpsCoordsHashMapPoint)> = grid_points
            .iter()
            .map(|p| {
                // https://rust-lang-nursery.github.io/rust-cookbook/science/mathematics/trigonometry.html#distance-between-two-points-on-the-earth
                let earth_radius_kilometer = 6371.0;
                let (possible_point_latitude_degrees, possible_point_longitude_degrees) =
                    (p.lat, p.lon);
                let (search_latitude_degrees, search_longitude_degrees) = (lat, lon);

                let possible_point_latitude = possible_point_latitude_degrees.to_radians();
                let search_latitude = search_latitude_degrees.to_radians();

                let delta_latitude =
                    (possible_point_latitude_degrees - search_latitude_degrees).to_radians();
                let delta_longitude =
                    (possible_point_longitude_degrees - search_longitude_degrees).to_radians();

                let central_angle_inner = (delta_latitude / 2.0).sin().powi(2)
                    + possible_point_latitude.cos()
                        * search_latitude.cos()
                        * (delta_longitude / 2.0).sin().powi(2);
                let central_angle = 2.0 * central_angle_inner.sqrt().asin();

                let distance = earth_radius_kilometer * central_angle;
                (distance.round() as u32, (**p).clone())
            })
            .collect();

        eprintln!("points with dist len {}", points_with_dist.len());

        points_with_dist.sort_by(|(dist_a, _), (dist_b, _)| dist_a.cmp(dist_b));

        points_with_dist.get(0).map(|(_, p)| p.clone())
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;

    #[test]
    fn closest_lookup() {
        println!("===================================");
        let tests: Vec<(Vec<GpsCoordsHashMapPoint>, GpsCoordsHashMapPoint, i64)> = vec![
            (
                vec![GpsCoordsHashMapPoint {
                    id: 1,
                    lat: 57.1640,
                    lon: 24.8652,
                }],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                },
                1,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.1640,
                        lon: 24.8652,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.1740,
                        lon: 24.8630,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: 57.1670,
                    lon: 24.8658,
                },
                1,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: 57.163429387682214,
                    lon: 24.87742424011231,
                },
                2,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: 57.193343289610794,
                    lon: 24.872531890869144,
                },
                1,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                2,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    GpsCoordsHashMapPoint {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                    GpsCoordsHashMapPoint {
                        id: 4,
                        lat: -36.03133177633188,
                        lon: -65.21484375000001,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                4,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    GpsCoordsHashMapPoint {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: -10.660607953624762,
                    lon: -52.03125,
                },
                3,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: 57.16961885299059,
                        lon: 24.875192642211914,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: 57.159484808175435,
                        lon: 24.877617359161377,
                    },
                    GpsCoordsHashMapPoint {
                        id: 3,
                        lat: 9.795677582829743,
                        lon: -1.7578125000000002,
                    },
                    GpsCoordsHashMapPoint {
                        id: 4,
                        lat: -36.03133177633188,
                        lon: -65.21484375000001,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: -28.92163128242129,
                    lon: 144.14062500000003,
                },
                4,
            ),
            (
                vec![
                    GpsCoordsHashMapPoint {
                        id: 1,
                        lat: -38.591187457054524,
                        lon: -156.33535699675508,
                    },
                    GpsCoordsHashMapPoint {
                        id: 2,
                        lat: -26.16538350360019,
                        lon: 34.914643003244926,
                    },
                ],
                GpsCoordsHashMapPoint {
                    id: 0,
                    lat: -28.92163128242129,
                    lon: 144.14062500000003,
                },
                1,
            ),
        ];
        for test in tests {
            println!("test case for coords");
            let (points, check_point, closest_id) = test;
            let mut coords = GpsCoordsHashMap::new();
            for point in points {
                coords.insert(point);
            }

            let closest = coords.get_closest(check_point.lat, check_point.lon);
            if let Some(closest) = closest {
                eprintln!("closest found id {} expected {}", closest.id, closest_id);
                assert_eq!(closest.id, closest_id);
            } else {
                panic!("No points found");
            }
        }
    }
}
