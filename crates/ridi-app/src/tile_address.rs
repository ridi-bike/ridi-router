#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileAddress {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileAddress {
    pub fn new(z: u8, x: u32, y: u32) -> Self {
        Self { z, x, y }
    }
}
