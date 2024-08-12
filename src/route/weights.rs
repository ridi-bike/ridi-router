use std::rc::Rc;

use geo::{HaversineBearing, Point};

use crate::map_data_graph::MapDataPointRef;

use super::{
    navigator::WeightCalcResult,
    segment::RouteSegment,
    segment_list::RouteSegmentList,
    walker::{RouteWalker, RouteWalkerMoveResult},
    Route,
};

pub struct WeightCalcInput<'a> {
    pub eval_fork_segment: &'a RouteSegment,
    pub route: &'a Route,
    pub all_fork_segments: &'a RouteSegmentList,
    pub start_point: MapDataPointRef,
    pub end_point: MapDataPointRef,
    pub fork_walker: RouteWalker<'a>,
}

pub type WeightCalc = fn(input: WeightCalcInput) -> WeightCalcResult;

pub fn weight_heading(input: WeightCalcInput) -> WeightCalcResult {
    let mut walker = input.fork_walker;
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
    let fork_line_one_geo = Point::new(
        fork_segment.get_line().borrow().points.0.borrow().lon,
        fork_segment.get_line().borrow().points.0.borrow().lat,
    );
    let fork_line_two_geo = Point::new(
        fork_segment.get_line().borrow().points.1.borrow().lon,
        fork_segment.get_line().borrow().points.1.borrow().lat,
    );
    let fork_bearing = if &fork_segment.get_line().borrow().points.1 == fork_segment.get_end_point()
    {
        fork_line_one_geo.haversine_bearing(fork_line_two_geo)
    } else {
        fork_line_two_geo.haversine_bearing(fork_line_one_geo)
    };

    let degree_offset_from_end = (fork_bearing - end_bearing).abs();

    let ratio: f64 = 255.0 / 180.0;
    // eprintln!(
    //     "point_id: {}, end_bearing: {}, fork_bearing: {}, ratio: {}, degree_offset: {}, res: {}",
    //     input.eval_fork_segment.get_end_point().borrow().id,
    //     end_bearing,
    //     fork_bearing,
    //     ratio,
    //     degree_offset_from_end,
    //     255.0 - (degree_offset_from_end * ratio).round()
    // );
    //

    WeightCalcResult::UseWithWeight(255 - (degree_offset_from_end * ratio).round() as u8)
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
        return WeightCalcResult::UseWithWeight(80);
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_no_loops(input: WeightCalcInput) -> WeightCalcResult {
    if input.route.has_looped() {
        return WeightCalcResult::DoNotUse;
    }

    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_check_distance_to_end(input: WeightCalcInput) -> WeightCalcResult {
    let check_steps_back = 100;

    let distance_to_end_current = match input.route.get_segment_last() {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment
            .get_end_point()
            .borrow()
            .distance_between(&input.end_point),
    };

    let distance_to_end_steps_back = match input.route.get_steps_from_end(check_steps_back) {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment
            .get_end_point()
            .borrow()
            .distance_between(&input.end_point),
    };

    if distance_to_end_current > distance_to_end_steps_back {
        return WeightCalcResult::DoNotUse;
    }
    WeightCalcResult::UseWithWeight(0)
}

pub fn weight_progress_speed(input: WeightCalcInput) -> WeightCalcResult {
    let check_steps_back = 100;

    let current_point = match input.route.get_segment_last() {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => segment.get_end_point(),
    };

    let total_distance = input
        .start_point
        .borrow()
        .distance_between(&input.end_point);
    let point_steps_back = match input.route.get_steps_from_end(check_steps_back) {
        None => return WeightCalcResult::UseWithWeight(0),
        Some(segment) => Rc::clone(segment.get_end_point()),
    };

    let average_distance_per_segment = total_distance / (input.route.get_segment_count() as f64);

    let distance_last_points = point_steps_back.borrow().distance_between(&current_point);
    let average_distance_last_points = distance_last_points / (check_steps_back as f64);

    if average_distance_last_points < average_distance_per_segment * 0.3 {
        // return WeightCalcResult::DoNotUse;
        return WeightCalcResult::UseWithWeight(0);
    }

    WeightCalcResult::UseWithWeight(0)
}

#[cfg(test)]
mod test {

    use crate::{
        debug_writer::DebugLoggerVoidSink,
        map_data_graph::MapDataPointRef,
        osm_data_reader::OsmDataReader,
        route::{
            navigator::WeightCalcResult, segment::RouteSegment, segment_list::RouteSegmentList,
            walker::RouteWalker,
        },
    };

    use super::{weight_heading, WeightCalcInput};

    fn get_route_segment(
        end_point: MapDataPointRef,
        opposite_point_for_line: MapDataPointRef,
    ) -> RouteSegment {
        let end_point_borrowed = end_point.borrow();
        let line = end_point_borrowed
            .lines
            .iter()
            .find(|line| {
                let line = line.borrow();
                (line.points.0 == end_point && line.points.1 == opposite_point_for_line)
                    || (line.points.1 == end_point && line.points.0 == opposite_point_for_line)
            })
            .expect("line to be found");

        RouteSegment::new(line.clone(), end_point.clone())
    }

    #[test]
    fn weight_heading_test() {
        let data_reader = OsmDataReader::new_file(String::from("src/test_data/sig-500.json"));
        let map_data = data_reader.read_data().expect("to load test file");
        let start = map_data
            .get_point_by_id(&885564366)
            .expect("to find start point");
        let end = map_data
            .get_point_by_id(&7535100633)
            .expect("to find end point");
        let disabled_debug_writer = Box::new(DebugLoggerVoidSink::default());
        let walker = RouteWalker::new(
            &map_data,
            start.clone(),
            end.clone(),
            disabled_debug_writer.clone(),
        );

        let fork_point = map_data
            .get_point_by_id(&81272994)
            .expect("to find fork point");

        let segment = get_route_segment(fork_point, start.clone());

        let fork_weight = weight_heading(WeightCalcInput {
            route: walker.get_route(),
            start_point: start.clone(),
            end_point: end.clone(),
            all_fork_segments: &RouteSegmentList::from(vec![]),
            eval_fork_segment: &segment,
            fork_walker: RouteWalker::new(
                &map_data,
                start.clone(),
                end.clone(),
                disabled_debug_writer.clone(),
            ),
        });
        eprintln!("{:#?}", fork_weight);

        let fork_point = map_data
            .get_point_by_id(&9212889586)
            .expect("to find fork point");

        let segment = get_route_segment(fork_point, start.clone());

        let fork_weight = weight_heading(WeightCalcInput {
            route: walker.get_route(),
            start_point: start.clone(),
            end_point: end.clone(),
            all_fork_segments: &RouteSegmentList::from(vec![]),
            eval_fork_segment: &segment,
            fork_walker: RouteWalker::new(
                &map_data,
                start.clone(),
                end.clone(),
                disabled_debug_writer.clone(),
            ),
        });
        eprintln!("{:#?}", fork_weight);
        assert_eq!(fork_weight, WeightCalcResult::DoNotUse);
    }
}
