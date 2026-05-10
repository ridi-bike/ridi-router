#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KeyboardPan {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl KeyboardPan {
    pub fn lat_fraction(self) -> f64 {
        match (self.up, self.down) {
            (true, false) => 0.02,
            (false, true) => -0.02,
            _ => 0.0,
        }
    }

    pub fn lon_fraction(self) -> f64 {
        match (self.left, self.right) {
            (true, false) => -0.02,
            (false, true) => 0.02,
            _ => 0.0,
        }
    }
}
