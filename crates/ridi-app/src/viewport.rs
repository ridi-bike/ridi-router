use crate::space::{GpsCoord, ScreenCoord, ScreenRect, Space, WorldBounds};
use crate::web_mercator_tiles::zoom_for_lon_span;

const WORLD_MIN_LAT: f64 = -90.0;
const WORLD_MAX_LAT: f64 = 90.0;
const WORLD_MIN_LON: f64 = -180.0;
const WORLD_MAX_LON: f64 = 180.0;

const MIN_LAT_SPAN: f64 = (WORLD_MAX_LAT - WORLD_MIN_LAT) / (1_u64 << 20) as f64;
const MIN_LON_SPAN: f64 = (WORLD_MAX_LON - WORLD_MIN_LON) / (1_u64 << 20) as f64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    center: GpsCoord,
    lat_span: f64,
    lon_span: f64,
}

impl Viewport {
    #[allow(dead_code)]
    pub fn full_world() -> Self {
        Self {
            center: GpsCoord { lat: 0.0, lon: 0.0 },
            lat_span: WORLD_MAX_LAT - WORLD_MIN_LAT,
            lon_span: WORLD_MAX_LON - WORLD_MIN_LON,
        }
    }

    pub fn centered_at_zoom(center: GpsCoord, zoom: u8) -> Self {
        let scale = 2.0_f64.powi(zoom as i32);
        let mut viewport = Self {
            center,
            lat_span: (WORLD_MAX_LAT - WORLD_MIN_LAT) / scale,
            lon_span: (WORLD_MAX_LON - WORLD_MIN_LON) / scale,
        };
        viewport.clamp_center();
        viewport
    }

    pub fn space(&self, screen: ScreenRect) -> Space {
        Space::new(self.world_bounds(), screen)
    }

    pub fn center(&self) -> GpsCoord {
        self.center
    }

    pub fn zoom_level(&self, max_zoom: u8) -> u8 {
        zoom_for_lon_span(self.lon_span, max_zoom)
    }

    pub fn pan_by_fraction(&mut self, lat_fraction: f64, lon_fraction: f64) {
        self.center.lat += self.lat_span * lat_fraction;
        self.center.lon += self.lon_span * lon_fraction;
        self.clamp_center();
    }

    pub fn pan_between_screen_points(
        &mut self,
        screen: ScreenRect,
        previous: ScreenCoord,
        current: ScreenCoord,
    ) {
        let space = self.space(screen);
        let previous_world = space.screen_to_world(previous);
        let current_world = space.screen_to_world(current);

        self.center.lat += previous_world.lat - current_world.lat;
        self.center.lon += previous_world.lon - current_world.lon;
        self.clamp_center();
    }

    pub fn zoom_around_screen_point(&mut self, screen: ScreenRect, anchor: ScreenCoord, zoom: f64) {
        if zoom == 1.0 {
            return;
        }

        let anchor_before_zoom = self.space(screen).screen_to_world(anchor);

        self.lat_span = (self.lat_span * zoom).clamp(MIN_LAT_SPAN, WORLD_MAX_LAT - WORLD_MIN_LAT);
        self.lon_span = (self.lon_span * zoom).clamp(MIN_LON_SPAN, WORLD_MAX_LON - WORLD_MIN_LON);
        self.clamp_center();

        let anchor_after_zoom = self.space(screen).screen_to_world(anchor);
        self.center.lat += anchor_before_zoom.lat - anchor_after_zoom.lat;
        self.center.lon += anchor_before_zoom.lon - anchor_after_zoom.lon;
        self.clamp_center();
    }

    pub fn world_bounds(&self) -> WorldBounds {
        WorldBounds {
            min: GpsCoord {
                lat: self.center.lat - self.lat_span / 2.0,
                lon: self.center.lon - self.lon_span / 2.0,
            },
            max: GpsCoord {
                lat: self.center.lat + self.lat_span / 2.0,
                lon: self.center.lon + self.lon_span / 2.0,
            },
        }
    }

    fn clamp_center(&mut self) {
        self.center.lat = self.center.lat.clamp(
            WORLD_MIN_LAT + self.lat_span / 2.0,
            WORLD_MAX_LAT - self.lat_span / 2.0,
        );
        self.center.lon = self.center.lon.clamp(
            WORLD_MIN_LON + self.lon_span / 2.0,
            WORLD_MAX_LON - self.lon_span / 2.0,
        );
    }
}
