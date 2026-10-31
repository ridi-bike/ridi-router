use crate::space::{
    lat_to_mercator_y, lon_to_mercator_x, mercator_x_to_lon, mercator_y_to_lat, GpsCoord,
    ScreenCoord, ScreenRect, Space, WorldBounds,
};

const TILE_SIZE_PX: f64 = 256.0;
const MIN_MERCATOR_UNITS_PER_PIXEL: f64 = 1.0 / ((1_u64 << 15) as f64 * TILE_SIZE_PX);
const MAX_MERCATOR_UNITS_PER_PIXEL: f64 = 1.0 / TILE_SIZE_PX;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    center: GpsCoord,
    mercator_units_per_pixel: f64,
}

impl Viewport {
    #[allow(dead_code)]
    pub fn full_world() -> Self {
        Self {
            center: GpsCoord { lat: 0.0, lon: 0.0 },
            mercator_units_per_pixel: MAX_MERCATOR_UNITS_PER_PIXEL,
        }
    }

    pub fn centered_at_zoom(center: GpsCoord, zoom: u8) -> Self {
        Self {
            center,
            mercator_units_per_pixel: mercator_units_per_pixel_for_zoom(zoom),
        }
    }

    pub fn space(&self, screen: ScreenRect) -> Space {
        Space::new(self.world_bounds_for_screen(screen), screen)
    }

    pub fn center(&self) -> GpsCoord {
        self.center
    }

    pub fn zoom_level(&self, max_zoom: u8) -> u8 {
        let zoom = (1.0 / (self.mercator_units_per_pixel * TILE_SIZE_PX))
            .log2()
            .floor();
        zoom.clamp(0.0, max_zoom as f64) as u8
    }

    pub fn pan_by_fraction(&mut self, screen: ScreenRect, lat_fraction: f64, lon_fraction: f64) {
        let center_x = lon_to_mercator_x(self.center.lon)
            + lon_fraction * screen.width * self.mercator_units_per_pixel;
        let center_y = lat_to_mercator_y(self.center.lat)
            - lat_fraction * screen.height * self.mercator_units_per_pixel;
        self.set_center_from_mercator(center_x, center_y);
        self.clamp_center(screen);
    }

    pub fn pan_between_screen_points(
        &mut self,
        screen: ScreenRect,
        previous: ScreenCoord,
        current: ScreenCoord,
    ) {
        let center_x = lon_to_mercator_x(self.center.lon)
            + (previous.x - current.x) * self.mercator_units_per_pixel;
        let center_y = lat_to_mercator_y(self.center.lat)
            + (previous.y - current.y) * self.mercator_units_per_pixel;
        self.set_center_from_mercator(center_x, center_y);
        self.clamp_center(screen);
    }

    pub fn zoom_around_screen_point(&mut self, screen: ScreenRect, anchor: ScreenCoord, zoom: f64) {
        if zoom == 1.0 {
            return;
        }

        let anchor_before_zoom = self.space(screen).screen_to_world(anchor);

        self.mercator_units_per_pixel = (self.mercator_units_per_pixel * zoom)
            .clamp(MIN_MERCATOR_UNITS_PER_PIXEL, MAX_MERCATOR_UNITS_PER_PIXEL);
        self.clamp_center(screen);

        let anchor_after_zoom = self.space(screen).screen_to_world(anchor);
        let center_x = lon_to_mercator_x(self.center.lon)
            + lon_to_mercator_x(anchor_before_zoom.lon)
            - lon_to_mercator_x(anchor_after_zoom.lon);
        let center_y = lat_to_mercator_y(self.center.lat)
            + lat_to_mercator_y(anchor_before_zoom.lat)
            - lat_to_mercator_y(anchor_after_zoom.lat);
        self.set_center_from_mercator(center_x, center_y);
        self.clamp_center(screen);
    }

    #[allow(dead_code)]
    pub fn world_bounds(&self) -> WorldBounds {
        self.world_bounds_for_screen(ScreenRect {
            x: 0.0,
            y: 0.0,
            width: TILE_SIZE_PX,
            height: TILE_SIZE_PX,
        })
    }

    fn world_bounds_for_screen(&self, screen: ScreenRect) -> WorldBounds {
        let center_x = lon_to_mercator_x(self.center.lon);
        let center_y = lat_to_mercator_y(self.center.lat);
        let x_span = screen.width * self.mercator_units_per_pixel;
        let y_span = screen.height * self.mercator_units_per_pixel;
        let min_x = (center_x - x_span / 2.0).clamp(0.0, 1.0);
        let max_x = (center_x + x_span / 2.0).clamp(0.0, 1.0);
        let min_y = (center_y - y_span / 2.0).clamp(0.0, 1.0);
        let max_y = (center_y + y_span / 2.0).clamp(0.0, 1.0);

        WorldBounds {
            min: GpsCoord {
                lat: mercator_y_to_lat(max_y),
                lon: mercator_x_to_lon(min_x),
            },
            max: GpsCoord {
                lat: mercator_y_to_lat(min_y),
                lon: mercator_x_to_lon(max_x),
            },
        }
    }

    fn set_center_from_mercator(&mut self, x: f64, y: f64) {
        self.center = GpsCoord {
            lat: mercator_y_to_lat(y),
            lon: mercator_x_to_lon(x),
        };
    }

    fn clamp_center(&mut self, screen: ScreenRect) {
        let half_x = (screen.width * self.mercator_units_per_pixel / 2.0).min(0.5);
        let half_y = (screen.height * self.mercator_units_per_pixel / 2.0).min(0.5);
        let center_x = lon_to_mercator_x(self.center.lon).clamp(half_x, 1.0 - half_x);
        let center_y = lat_to_mercator_y(self.center.lat).clamp(half_y, 1.0 - half_y);
        self.set_center_from_mercator(center_x, center_y);
    }
}

fn mercator_units_per_pixel_for_zoom(zoom: u8) -> f64 {
    (1.0 / (2.0_f64.powi(zoom as i32) * TILE_SIZE_PX))
        .clamp(MIN_MERCATOR_UNITS_PER_PIXEL, MAX_MERCATOR_UNITS_PER_PIXEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_preserves_mercator_scale_for_wide_screen() {
        let viewport = Viewport {
            center: GpsCoord { lat: 0.0, lon: 0.0 },
            mercator_units_per_pixel: 0.000_1,
        };
        let screen = ScreenRect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 500.0,
        };
        let space = viewport.space(screen);
        let center = space.world_to_screen(viewport.center);

        let east = space.world_to_screen(GpsCoord { lat: 0.0, lon: 3.6 });
        let south_lat = mercator_y_to_lat(lat_to_mercator_y(0.0) + 0.01);
        let south = space.world_to_screen(GpsCoord {
            lat: south_lat,
            lon: 0.0,
        });

        assert!(((east.x - center.x) - (south.y - center.y)).abs() < 1e-9);
    }

    #[test]
    fn resizing_keeps_pixels_per_mercator_unit() {
        let viewport = Viewport {
            center: GpsCoord { lat: 0.0, lon: 0.0 },
            mercator_units_per_pixel: 0.000_1,
        };
        let previous = ScreenRect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 500.0,
        };
        let current = ScreenRect {
            x: 0.0,
            y: 0.0,
            width: 1500.0,
            height: 500.0,
        };
        let previous_space = viewport.space(previous);
        let previous_center = previous_space.world_to_screen(viewport.center);
        let previous_east = previous_space.world_to_screen(GpsCoord { lat: 0.0, lon: 3.6 });

        let current_space = viewport.space(current);
        let current_center = current_space.world_to_screen(viewport.center);
        let current_east = current_space.world_to_screen(GpsCoord { lat: 0.0, lon: 3.6 });

        assert!(
            ((previous_east.x - previous_center.x) - (current_east.x - current_center.x)).abs()
                < 1e-9
        );
    }
}
