pub fn zoom_factor(wheel_y: f32, zoom_out_key_down: bool, zoom_in_key_down: bool) -> f64 {
    let key_zoom = if zoom_out_key_down {
        1.02
    } else if zoom_in_key_down {
        0.98
    } else {
        1.0
    };

    let wheel_zoom = if wheel_y > 0.0 {
        0.9
    } else if wheel_y < 0.0 {
        1.1
    } else {
        1.0
    };

    key_zoom * wheel_zoom
}
