use geo::{Bearing, Haversine, Point};

use crate::router::generator::RouteWithStats;

use super::Route;

pub struct Score;

impl Score {
    pub fn calc_score(route: &Route) -> f64 {
        let mut prev_bearing: Option<f32> = None;
        let mut tot_bearing_diff: f64 = 0.;
        let mut len_m: f64 = 0.;

        for segment in route.iter() {
            let line_len: f64 = segment.get_line().borrow().get_len_m().into();
            len_m += line_len;

            let line = segment.get_line().borrow();
            let point_1 = line.points.0.borrow();
            let point_2 = line.points.1.borrow();
            let geo_point_1 = Point::new(point_1.lon, point_1.lat);
            let geo_point_2 = Point::new(point_2.lon, point_2.lat);
            let curr_bearing = if line.points.0 == *segment.get_end_point() {
                Haversine::bearing(geo_point_1, geo_point_2)
                // geo_point_1.haversine_bearing(geo_point_2)
            } else {
                // geo_point_2.haversine_bearing(geo_point_1)
                Haversine::bearing(geo_point_2, geo_point_1)
            };
            if let Some(prev_bearing) = prev_bearing {
                let bearing_diff = (prev_bearing - curr_bearing).abs() as f64;
                tot_bearing_diff += if bearing_diff >= 90. {
                    // assumption is that a 90 or more
                    // degree turn is a junction, not a curve
                    // we don't want junctions
                    0.
                } else {
                    bearing_diff
                };
                // tot_bearing_diff += bearing_diff;
            }
            prev_bearing = if segment.get_end_point().borrow().is_junction() {
                None
            } else if let Some(hw) = line.tags.borrow().highway() {
                if hw == "residential" {
                    None
                } else {
                    Some(curr_bearing)
                }
            } else {
                Some(curr_bearing)
            };
        }

        tot_bearing_diff / len_m * 1000.
    }
}

#[cfg(test)]
mod test {}
