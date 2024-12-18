use std::{collections::HashMap, fmt::Write};

use super::route::Route;
use ndarray::{Array, ArrayView};
use petal_clustering::{Fit, HDbscan};
use serde::{Deserialize, Serialize};

const APPROX_POINTS: usize = 10;

#[derive(Serialize, Deserialize, Debug)]
pub struct Clustering {
    pub approximated_routes: Vec<Vec<[f32; 2]>>,
    pub clustering: (HashMap<usize, Vec<usize>>, Vec<usize>),
}

impl Clustering {
    pub fn generate(routes: &Vec<Route>) -> Self {
        let mut approximated_routes = Vec::new();
        let mut point_array = Array::zeros((0, 20));

        for route in routes {
            let points_in_step = route.get_segment_count() as f32 / APPROX_POINTS as f32;
            let approximated_points = (0..APPROX_POINTS as u32)
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
            point_array
                .push_row(ArrayView::from(approximated_points.as_flattened()))
                .unwrap();
            approximated_routes.push(approximated_points);
        }

        let clustering = HDbscan::default().fit(&point_array);

        Self {
            approximated_routes,
            clustering,
        }
    }
}
