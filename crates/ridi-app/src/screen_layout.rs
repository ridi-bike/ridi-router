use crate::space::ScreenRect;

pub fn padded_screen_rect(
    screen_width: f32,
    screen_height: f32,
    padding_fraction: f32,
) -> ScreenRect {
    let padding_x = screen_width * padding_fraction;
    let padding_y = screen_height * padding_fraction;

    ScreenRect {
        x: padding_x as f64,
        y: padding_y as f64,
        width: (screen_width - padding_x * 2.0) as f64,
        height: (screen_height - padding_y * 2.0) as f64,
    }
}
