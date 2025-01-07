use geo::Point;
use gpx::{write, Gpx, GpxVersion, Route as GpxRoute, Track, TrackSegment, Waypoint};
use std::{collections::HashMap, fs::File, io::Error, isize, path::PathBuf};

use crate::{
    ipc_handler::RouteMessage,
    router::{
        itinerary::Itinerary,
        route::{segment::Segment, RouteStatElement},
    },
};

#[derive(Debug, thiserror::Error)]
pub enum GpxWriterError {
    #[error("File Creation Error cause {error}")]
    FileCreateError { error: Error },
}

pub struct GpxWriter {
    routes: Vec<RouteMessage>,
    file_name: PathBuf,
}

fn sort_by_longest(map: HashMap<String, RouteStatElement>) -> Vec<(String, RouteStatElement)> {
    let mut vec = Vec::from_iter(map.into_iter());
    vec.sort_by(|a, b| b.1.len_m.total_cmp(&a.1.len_m));
    vec
}

pub fn write_debug_itinerary(idx: usize, itinerary: &Itinerary) -> () {
    let mut gpx = Gpx::default();
    gpx.version = GpxVersion::Gpx11;
    let geo_p = Point::new(
        itinerary.start.borrow().lon as f64,
        itinerary.start.borrow().lat as f64,
    );
    let mut wp = Waypoint::new(geo_p);
    wp.name = Some("Start".to_string());
    gpx.waypoints.push(wp);
    let geo_p = Point::new(
        itinerary.finish.borrow().lon as f64,
        itinerary.finish.borrow().lat as f64,
    );
    let mut wp = Waypoint::new(geo_p);
    wp.name = Some("Finish".to_string());
    gpx.waypoints.push(wp);

    for (wp_idx, wp) in itinerary.waypoints.iter().enumerate() {
        let geo_p = Point::new(wp.borrow().lon as f64, wp.borrow().lat as f64);
        let mut wp = Waypoint::new(geo_p);
        wp.name = Some(format!("wp {wp_idx}"));
        gpx.waypoints.push(wp);
    }
    let debug_filename = PathBuf::from(format!("/tmp/{}.wp.gpx", idx));
    let debug_file = File::create(debug_filename)
        .or_else(|error| Err(GpxWriterError::FileCreateError { error }))
        .unwrap();

    write(&gpx, debug_file).unwrap();
}

pub fn write_debug_segment(idx: usize, all_segments: Vec<Segment>, route: Vec<Segment>) -> () {
    let mut gpx = Gpx::default();
    gpx.version = GpxVersion::Gpx11;

    for segment in all_segments {
        let mut track = Track::new();
        let mut track_segment = TrackSegment::new();
        track_segment.points.push(Waypoint::new(Point::new(
            segment.get_line().borrow().points.0.borrow().lon as f64,
            segment.get_line().borrow().points.0.borrow().lat as f64,
        )));
        track_segment.points.push(Waypoint::new(Point::new(
            segment.get_line().borrow().points.1.borrow().lon as f64,
            segment.get_line().borrow().points.1.borrow().lat as f64,
        )));
        track.segments.push(track_segment);

        gpx.tracks.push(track);
    }
    let mut gpx_route = GpxRoute::new();
    for segment in route {
        gpx_route.points.push(Waypoint::new(Point::new(
            segment.get_end_point().borrow().lon as f64,
            segment.get_end_point().borrow().lat as f64,
        )));
    }
    gpx.routes.push(gpx_route);
    let debug_filename = PathBuf::from(format!("/tmp/{}.seg.gpx", idx));
    let debug_file = File::create(debug_filename)
        .or_else(|error| Err(GpxWriterError::FileCreateError { error }))
        .unwrap();

    write(&gpx, debug_file).unwrap();
}

impl GpxWriter {
    pub fn new(routes: Vec<RouteMessage>, file_name: PathBuf) -> Self {
        Self { routes, file_name }
    }
    pub fn write_gpx(self) -> Result<(), GpxWriterError> {
        let mut gpx = Gpx::default();
        gpx.version = GpxVersion::Gpx11;

        let mut csv_contents = String::from("id,len,junctions,score,cluster\n");
        for (idx, route) in self.routes.clone().into_iter().enumerate() {
            csv_contents.push_str(&format!(
                "r_{},{},{},{},{}\n",
                idx,
                route.stats.len_m / 1000.,
                route.stats.junction_count,
                route.stats.score,
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));
            let mut gpx_route = GpxRoute::new();
            gpx_route.name = Some(format!(
                "r_{idx}_c_{}",
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));

            let mut description = String::new();
            description.push_str(&format!("Length: {:.2}km\n", route.stats.len_m / 1000.));
            description.push_str(&format!(
                "Number of junctions: {}\n",
                route.stats.junction_count
            ));
            description.push_str(&format!(
                "Cluster: {}\n",
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));
            description.push_str(&format!("Score: {:.2}\n", route.stats.score));
            description.push_str(&format!("Road types:\n"));
            for (road_type, stat) in sort_by_longest(route.stats.highway).iter() {
                description.push_str(&format!(
                    " - {road_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }
            description.push_str(&format!("Road surface:\n"));
            for (surface_type, stat) in sort_by_longest(route.stats.surface).iter() {
                description.push_str(&format!(
                    " - {surface_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }
            description.push_str(&format!("Road smoothness:\n"));
            for (smoothness_type, stat) in sort_by_longest(route.stats.smoothness).iter() {
                description.push_str(&format!(
                    " - {smoothness_type}: {:.2}km, {:.2}%\n",
                    stat.len_m / 1000.,
                    stat.percentage,
                ));
            }

            gpx_route.description = Some(description);

            for (lat, lon) in &route.coords {
                let waypoint = Waypoint::new(Point::new(*lon as f64, *lat as f64));
                gpx_route.points.push(waypoint);
            }

            gpx.routes.push(gpx_route);
        }

        let mut csv_filename = PathBuf::from(&self.file_name);
        csv_filename.set_extension("csv");
        std::fs::write(csv_filename, csv_contents).unwrap();
        let file = File::create(&self.file_name)
            .or_else(|error| Err(GpxWriterError::FileCreateError { error }))?;

        write(&gpx, file).unwrap();

        let mut gpx_approx = Gpx::default();
        gpx_approx.version = GpxVersion::Gpx11;

        for (idx, route) in self.routes.into_iter().enumerate() {
            let mut gpx_route = GpxRoute::new();
            gpx_route.name = Some(format!(
                "r_{idx}_c_{}",
                route.stats.cluster.map_or(-1, |c| c as isize)
            ));
            for coord in &route.stats.approximated_route {
                let waypoint = Waypoint::new(Point::new(coord.1.into(), coord.0.into()));
                gpx_route.points.push(waypoint);
            }
            gpx_approx.routes.push(gpx_route);
        }

        let mut gpx_approx_filename = PathBuf::from(&self.file_name);
        gpx_approx_filename.set_extension("approx.gpx");
        let gpx_approx_file = File::create(gpx_approx_filename)
            .or_else(|error| Err(GpxWriterError::FileCreateError { error }))?;

        write(&gpx_approx, gpx_approx_file).unwrap();

        Ok(())
    }
}
