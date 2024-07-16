pub fn get_distance(from_lat: &f64, from_lon: &f64, to_lat: &f64, to_lon: &f64) -> f64 {
    // https://rust-lang-nursery.github.io/rust-cookbook/science/mathematics/trigonometry.html#distance-between-two-points-on-the-earth
    let earth_radius_kilometer = 6371.0;

    let from_lat_rad = from_lat.to_radians();
    let to_lat_rad = to_lat.to_radians();

    let delta_latitude = (from_lat - to_lat).to_radians();
    let delta_longitude = (from_lon - to_lon).to_radians();

    let central_angle_inner = (delta_latitude / 2.0).sin().powi(2)
        + from_lat_rad.cos() * to_lat_rad.cos() * (delta_longitude / 2.0).sin().powi(2);
    let central_angle = 2.0 * central_angle_inner.sqrt().asin();

    earth_radius_kilometer * central_angle
}

pub fn get_heading(from_lat: &f64, from_lon: &f64, to_lat: &f64, to_lon: &f64) -> f64 {
    // https://www.ridgesolutions.ie/index.php/2022/05/26/code-to-calculate-heading-bearing-from-two-gps-latitude-and-longitude/
    let from_lat_rad = from_lat.to_radians();
    let from_lon_rad = from_lon.to_radians();
    let to_lat_rad = to_lat.to_radians();
    let to_lon_rad = to_lon.to_radians();

    let delta_lon = to_lon_rad - from_lon_rad;
    let x = to_lat_rad.cos() * delta_lon.sin();
    let y = from_lat_rad.cos() * to_lat_rad.cos()
        - from_lat_rad.sin() * to_lat_rad.cos() * delta_lon.cos();

    let heading = x.atan2(y);
    let heading = heading.to_degrees();
    let heading = if heading < 0.0 {
        heading + 360.0
    } else {
        heading
    };

    heading
}
