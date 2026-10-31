const MERCATOR_MAX_LAT: f64 = 85.051_128_78;
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsCoord {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenCoord {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: GpsCoord,
    pub max: GpsCoord,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Space {
    pub world: WorldBounds,
    pub screen: ScreenRect,
}

impl Space {
    pub fn new(world: WorldBounds, screen: ScreenRect) -> Self {
        Self { world, screen }
    }

    pub fn world_to_screen(&self, gps: GpsCoord) -> ScreenCoord {
        let min_x = lon_to_mercator_x(self.world.min.lon);
        let max_x = lon_to_mercator_x(self.world.max.lon);
        let min_y = lat_to_mercator_y(self.world.max.lat);
        let max_y = lat_to_mercator_y(self.world.min.lat);
        let x_range = max_x - min_x;
        let y_range = max_y - min_y;

        let x =
            self.screen.x + ((lon_to_mercator_x(gps.lon) - min_x) / x_range) * self.screen.width;
        let y =
            self.screen.y + ((lat_to_mercator_y(gps.lat) - min_y) / y_range) * self.screen.height;

        ScreenCoord { x, y }
    }

    pub fn screen_to_world(&self, screen: ScreenCoord) -> GpsCoord {
        let min_x = lon_to_mercator_x(self.world.min.lon);
        let max_x = lon_to_mercator_x(self.world.max.lon);
        let min_y = lat_to_mercator_y(self.world.max.lat);
        let max_y = lat_to_mercator_y(self.world.min.lat);
        let x_range = max_x - min_x;
        let y_range = max_y - min_y;

        let mercator_x = min_x + ((screen.x - self.screen.x) / self.screen.width) * x_range;
        let mercator_y = min_y + ((screen.y - self.screen.y) / self.screen.height) * y_range;

        GpsCoord {
            lat: mercator_y_to_lat(mercator_y),
            lon: mercator_x_to_lon(mercator_x),
        }
    }

    pub fn world_to_map_screen(
        &self,
        gps: GpsCoord,
        screen_width: f32,
        screen_height: f32,
    ) -> ScreenCoord {
        self.mercator_to_map_screen(
            lon_to_mercator_x(gps.lon),
            lat_to_mercator_y(gps.lat),
            screen_width,
            screen_height,
        )
    }

    pub fn mercator_to_map_screen(
        &self,
        mercator_x: f64,
        mercator_y: f64,
        screen_width: f32,
        screen_height: f32,
    ) -> ScreenCoord {
        let min_x = lon_to_mercator_x(self.world.min.lon);
        let max_x = lon_to_mercator_x(self.world.max.lon);
        let min_y = lat_to_mercator_y(self.world.max.lat);
        let max_y = lat_to_mercator_y(self.world.min.lat);

        ScreenCoord {
            x: ((mercator_x - min_x) / (max_x - min_x)) * screen_width as f64,
            y: ((mercator_y - min_y) / (max_y - min_y)) * screen_height as f64,
        }
    }

    pub fn map_screen_to_world(
        &self,
        screen: ScreenCoord,
        screen_width: f32,
        screen_height: f32,
    ) -> GpsCoord {
        let min_x = lon_to_mercator_x(self.world.min.lon);
        let max_x = lon_to_mercator_x(self.world.max.lon);
        let min_y = lat_to_mercator_y(self.world.max.lat);
        let max_y = lat_to_mercator_y(self.world.min.lat);
        let x_range = max_x - min_x;
        let y_range = max_y - min_y;

        let mercator_x = min_x + (screen.x / screen_width as f64) * x_range;
        let mercator_y = min_y + (screen.y / screen_height as f64) * y_range;

        GpsCoord {
            lat: mercator_y_to_lat(mercator_y),
            lon: mercator_x_to_lon(mercator_x),
        }
    }
}

pub fn lon_to_mercator_x(lon: f64) -> f64 {
    (lon + 180.0) / 360.0
}

pub fn mercator_x_to_lon(x: f64) -> f64 {
    x * 360.0 - 180.0
}

pub fn lat_to_mercator_y(lat: f64) -> f64 {
    let lat = lat.clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT).to_radians();
    (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0
}

pub fn mercator_y_to_lat(y: f64) -> f64 {
    let lat_rad = (std::f64::consts::PI * (1.0 - 2.0 * y)).sinh().atan();
    lat_rad.to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_screen_round_trip() {
        let space = Space::new(
            WorldBounds {
                min: GpsCoord {
                    lat: 50.0,
                    lon: 10.0,
                },
                max: GpsCoord {
                    lat: 60.0,
                    lon: 20.0,
                },
            },
            ScreenRect {
                x: 100.0,
                y: 50.0,
                width: 800.0,
                height: 600.0,
            },
        );

        let gps = GpsCoord {
            lat: 52.5,
            lon: 12.5,
        };
        let screen = space.world_to_screen(gps);
        let round_trip = space.screen_to_world(screen);

        assert!((gps.lat - round_trip.lat).abs() < 1e-12);
        assert!((gps.lon - round_trip.lon).abs() < 1e-12);
    }
}
