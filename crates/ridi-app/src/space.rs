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
        let lon_range = self.world.max.lon - self.world.min.lon;
        let lat_range = self.world.max.lat - self.world.min.lat;

        let x = self.screen.x + ((gps.lon - self.world.min.lon) / lon_range) * self.screen.width;
        let y = self.screen.y + ((self.world.max.lat - gps.lat) / lat_range) * self.screen.height;

        ScreenCoord { x, y }
    }

    pub fn screen_to_world(&self, screen: ScreenCoord) -> GpsCoord {
        let lon_range = self.world.max.lon - self.world.min.lon;
        let lat_range = self.world.max.lat - self.world.min.lat;

        let lon = self.world.min.lon + ((screen.x - self.screen.x) / self.screen.width) * lon_range;
        let lat =
            self.world.max.lat - ((screen.y - self.screen.y) / self.screen.height) * lat_range;

        GpsCoord { lat, lon }
    }
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

        assert!((gps.lat - round_trip.lat).abs() < f64::EPSILON);
        assert!((gps.lon - round_trip.lon).abs() < f64::EPSILON);
    }
}
