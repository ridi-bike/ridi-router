use crate::space::GpsCoord;
use crate::tile_address::TileAddress;

pub fn lon_lat_to_tile(coord: GpsCoord, zoom: u8) -> TileAddress {
    let n = 2.0_f64.powi(zoom as i32);
    let lat = coord.lat.clamp(-85.051_128_78, 85.051_128_78).to_radians();
    let x = ((coord.lon + 180.0) / 360.0 * n)
        .floor()
        .clamp(0.0, n - 1.0) as u32;
    let y = ((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor()
        .clamp(0.0, n - 1.0) as u32;

    TileAddress::new(zoom, x, y)
}

pub fn zoom_for_lon_span(lon_span: f64, max_zoom: u8) -> u8 {
    let tiles_across = (360.0 / lon_span.max(f64::EPSILON)).max(1.0);
    tiles_across.log2().floor().clamp(0.0, max_zoom as f64) as u8
}
