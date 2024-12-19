use super::route::Route;
use hdbscan::{Hdbscan, HdbscanHyperParams};
use serde::{Deserialize, Serialize};

const APPROXIMATION_POINTS: usize = 10;

#[derive(Serialize, Deserialize, Debug)]
pub struct Clustering {
    pub approximated_routes: Vec<Vec<[f32; 2]>>,
    pub labels: Vec<i32>,
}

impl Clustering {
    pub fn generate(routes: &Vec<Route>) -> Self {
        let mut approximated_routes = Vec::new();
        // let mut point_array = Array::zeros((0, 2 * APPROXIMATION_POINTS));
        let mut points = Vec::new();

        for route in routes {
            let points_in_step = route.get_segment_count() as f32 / APPROXIMATION_POINTS as f32;
            let approximated_points = (0..APPROXIMATION_POINTS as u32)
                .map(|step| {
                    let route_chunk = route.get_route_chunk(
                        (step as f32 * points_in_step) as usize,
                        ((step as f32 + 1.) * points_in_step) as usize,
                    );
                    let sum_point = route_chunk
                        .iter()
                        .map(|s| {
                            (
                                s.get_end_point().borrow().lat,
                                s.get_end_point().borrow().lon,
                            )
                        })
                        .fold((0., 0.), |acc, el| (acc.0 + el.0, acc.1 + el.1));
                    [
                        sum_point.0 / route_chunk.len() as f32,
                        sum_point.1 / route_chunk.len() as f32,
                    ]
                })
                .collect::<Vec<_>>();
            points.push(approximated_points.as_flattened().to_vec());
            approximated_routes.push(approximated_points);
        }

        let params = HdbscanHyperParams::builder()
            .epsilon(0.05)
            .min_cluster_size(1)
            .build();
        let alg = Hdbscan::new(&points, params);
        let labels = alg.cluster().unwrap();

        Self {
            approximated_routes,
            labels,
        }
    }
}
