use std::rc::Rc;

use geo::{HaversineBearing, Point};

use crate::map_data_graph::{MapDataPoint, MapDataPointRef};

use super::{
    navigator::WeightCalcResult,
    walker::{Route, RouteSegment, RouteSegmentList, RouteWalker, RouteWalkerMoveResult},
};

pub struct WeightCalcInput<'a> {
    pub eval_fork_segment: &'a RouteSegment,
    pub route: &'a Route,
    pub all_fork_segments: &'a RouteSegmentList,
    pub start_point: MapDataPointRef,
    pub end_point: MapDataPointRef,
    pub walker: RouteWalker<'a>,
}

pub type WeightCalc = fn(input: WeightCalcInput) -> WeightCalcResult;

pub fn weight_heading(input: WeightCalcInput) -> WeightCalcResult {
    let mut walker = input.walker;
    let next_fork = match walker.move_forward_to_next_fork() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("weight calc error {:#?}", e);
            return WeightCalcResult::DoNotUse;
        }
    };
    let _ = match next_fork {
        RouteWalkerMoveResult::DeadEnd => return WeightCalcResult::DoNotUse,
        RouteWalkerMoveResult::Finish => return WeightCalcResult::UseWithWeight(255),
        RouteWalkerMoveResult::Fork(f) => f,
    };
    let fork_segment = match walker.get_route().get_segment_last() {
        Some(last_segment) => last_segment,
        None => input.eval_fork_segment,
    };
    let fork_point_geo = Point::new(
        fork_segment.get_end_point().borrow().lon,
        fork_segment.get_end_point().borrow().lat,
    );
    let end_point_geo = Point::new(input.end_point.borrow().lon, input.end_point.borrow().lat);
    let end_bearing = fork_point_geo.haversine_bearing(end_point_geo);
    // let (fork_point_one, fork_point_two) =
    //     if &fork_segment.get_line().borrow().points.0 == fork_segment.get_end_point() {
    //         (
    //             Rc::clone(&fork_segment.get_line().borrow().points.1),
    //             Rc::clone(&fork_segment.get_line().borrow().points.0),
    //         )
    //     } else {
    //         (
    //             Rc::clone(&fork_segment.get_line().borrow().points.0),
    //             Rc::clone(&fork_segment.get_line().borrow().points.1),
    //         )
    //     };
    let fork_point_one_geo = Point::new(
        fork_segment.get_line().borrow().points.0.borrow().lon,
        fork_segment.get_line().borrow().points.0.borrow().lat,
    );
    let fork_point_two_geo = Point::new(
        fork_segment.get_line().borrow().points.1.borrow().lon,
        fork_segment.get_line().borrow().points.1.borrow().lat,
    );
    let fork_bearing = if &fork_segment.get_line().borrow().points.1 == fork_segment.get_end_point()
    {
        fork_point_one_geo.haversine_bearing(fork_point_two_geo)
    } else {
        fork_point_two_geo.haversine_bearing(fork_point_one_geo)
    };

    match end_bearing - fork_bearing {
        -15.0..15.0 => WeightCalcResult::UseWithWeight(150),
        -45.0..-12.0 => WeightCalcResult::UseWithWeight(100),
        15.0..45.0 => WeightCalcResult::UseWithWeight(100),
        -90.0..45.0 => WeightCalcResult::UseWithWeight(50),
        45.0..90.0 => WeightCalcResult::UseWithWeight(50),
        -135.0..-90.0 => WeightCalcResult::UseWithWeight(10),
        90.0..135.0 => WeightCalcResult::UseWithWeight(10),
        _ => WeightCalcResult::UseWithWeight(1),
    }
}

pub fn weight_prefer_same_road(input: WeightCalcInput) -> WeightCalcResult {
    let current_ref = input
        .route
        .get_segment_last()
        .map_or(None, |s| s.get_line().borrow().tags_ref.clone());
    let current_name = input
        .route
        .get_segment_last()
        .map_or(None, |s| s.get_line().borrow().tags_name.clone());
    let fork_ref = input.eval_fork_segment.get_line().borrow().tags_ref.clone();
    let fork_name = input
        .eval_fork_segment
        .get_line()
        .borrow()
        .tags_name
        .clone();

    if current_ref == fork_ref || current_name == fork_name {
        return WeightCalcResult::UseWithWeight(150);
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_no_loops(input: WeightCalcInput) -> WeightCalcResult {
    if input
        .route
        .contains_point_id(input.eval_fork_segment.get_end_point().borrow().id)
    {
        return WeightCalcResult::DoNotUse;
    }

    WeightCalcResult::UseWithWeight(0)
}

#[cfg(test)]
mod test {

    use std::{cell::RefCell, rc::Rc};

    use crate::{
        map_data_graph::{MapDataLine, MapDataPoint, MapDataWay, MapDataWayPoints},
        route::{
            navigator::WeightCalcResult,
            walker::{Route, RouteSegment, RouteSegmentList},
        },
    };

    use super::{weight_heading, WeightCalcInput};

    #[test]
    fn weight_heading_test() {
        let route = Route::new();
        let end_point = Rc::new(RefCell::new(MapDataPoint {
            // 57.15651, 24.84966
            id: 2,
            lat: 57.15651,
            lon: 24.84966,
            junction: false,
            lines: Vec::new(),
            part_of_ways: Vec::new(),
            rules: Vec::new(),
        }));
        let start_point = Rc::new(RefCell::new(MapDataPoint {
            // 57.15471, 24.84954
            id: 1,
            lat: 57.15471,
            lon: 24.84954,
            junction: true,
            lines: Vec::new(),
            part_of_ways: Vec::new(),
            rules: Vec::new(),
        }));
        let way = Rc::new(RefCell::new(MapDataWay {
            id: 1,
            points: MapDataWayPoints::from_vec(vec![
                Rc::clone(&start_point),
                Rc::clone(&end_point),
            ]),
        }));
        let choice_segment = RouteSegment::new(
            Rc::new(RefCell::new(MapDataLine {
                id: String::from("1"),
                points: (Rc::clone(&start_point), Rc::clone(&start_point)),
                way: Rc::clone(&way),
                one_way: false, // one_way: false,
                                // length_m: 0.0,
                                // bearing_deg: 0.0,
            })),
            Rc::new(RefCell::new(MapDataPoint {
                // 57.15514, 24.85033
                id: 3,
                lat: 57.15514,
                lon: 24.85033,
                junction: false,
                lines: Vec::new(),
                part_of_ways: Vec::new(),
                rules: Vec::new(),
            })),
        );
        let all_choice_segments = RouteSegmentList::new();
        let weight = weight_heading(WeightCalcInput {
            route: &route,
            eval_fork_segment: &choice_segment,
            all_fork_segments: &all_choice_segments,
            start_point: Rc::clone(&start_point),
            end_point: Rc::clone(&end_point),
        });

        if let WeightCalcResult::UseWithWeight(weight) = weight {
            assert_eq!(weight, 100);
        } else {
            assert!(false);
        }
    }
}
