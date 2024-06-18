use std::{collections::BTreeMap, rc::Rc};

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

        for level in 0..=64 {
            eprintln!("level {}", level);
            let square_size = if level == 0 { 0 } else { 2_u64.pow(level) };
            let from = if search_hash >= square_size {
                search_hash - square_size
            } else {
                0
            };
            let to = search_hash;
            eprintln!("square_size {}", square_size);
            eprintln!("range {}..={}", from, to);

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
            if !points.is_empty() || from == 0 {
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
