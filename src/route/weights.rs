use geo::{HaversineBearing, Point};

use crate::map_data_graph::{MapDataPoint, MapDataPointRef};

use super::{
    navigator::WeightCalcResult,
    walker::{Route, RouteSegment, RouteSegmentList},
};

pub struct WeightCalcInput<'a> {
    pub choice_segment: &'a RouteSegment,
    pub route: &'a Route,
    pub all_choice_segments: &'a RouteSegmentList,
    pub start_point: MapDataPointRef,
    pub end_point: MapDataPointRef,
}

pub type WeightCalc = fn(input: WeightCalcInput) -> WeightCalcResult;

pub fn weight_heading(input: WeightCalcInput) -> WeightCalcResult {
    let choice_point = match input.route.get_segment_last() {
        Some(segment) => segment.get_end_point(),
        None => &input.start_point,
    };
    let choice_point_geo = Point::new(choice_point.borrow().lon, choice_point.borrow().lat);
    let end_point_geo = Point::new(input.end_point.borrow().lon, input.end_point.borrow().lat);
    let end_heading = choice_point_geo.haversine_bearing(end_point_geo);
    let choice_segment_point_geo = Point::new(
        input.choice_segment.get_end_point().borrow().lon,
        input.choice_segment.get_end_point().borrow().lat,
    );
    let choice_heading = choice_point_geo.haversine_bearing(choice_segment_point_geo);
    match end_heading - choice_heading {
        -15.0..15.0 => WeightCalcResult::UseWithWeight(150),
        -45.0..-12.0 => WeightCalcResult::UseWithWeight(100),
        15.0..45.0 => WeightCalcResult::UseWithWeight(100),
        -90.0..45.0 => WeightCalcResult::UseWithWeight(50),
        45.0..90.0 => WeightCalcResult::UseWithWeight(50),
        -135.0..-90.0 => WeightCalcResult::UseWithWeight(1),
        90.0..135.0 => WeightCalcResult::UseWithWeight(1),
        _ => WeightCalcResult::DoNotUse,
    }
}

pub fn weight_no_loops(input: WeightCalcInput) -> WeightCalcResult {
    if input
        .route
        .contains_point_id(input.choice_segment.get_end_point().borrow().id)
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
            fork: false,
            lines: Vec::new(),
            part_of_ways: Vec::new(),
        }));
        let start_point = Rc::new(RefCell::new(MapDataPoint {
            // 57.15471, 24.84954
            id: 1,
            lat: 57.15471,
            lon: 24.84954,
            fork: true,
            lines: Vec::new(),
            part_of_ways: Vec::new(),
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
                // one_way: false,
                // length_m: 0.0,
                // bearing_deg: 0.0,
            })),
            Rc::new(RefCell::new(MapDataPoint {
                // 57.15514, 24.85033
                id: 3,
                lat: 57.15514,
                lon: 24.85033,
                fork: false,
                lines: Vec::new(),
                part_of_ways: Vec::new(),
            })),
        );
        let all_choice_segments = RouteSegmentList::new();
        let weight = weight_heading(WeightCalcInput {
            route: &route,
            choice_segment: &choice_segment,
            all_choice_segments: &all_choice_segments,
            start_point: Rc::clone(&start_point),
            end_point: Rc::clone(&end_point),
        });

        if let WeightCalcResult::UseWithWeight(weight) = weight {
            assert_eq!(weight, 10);
        }
    }
}
