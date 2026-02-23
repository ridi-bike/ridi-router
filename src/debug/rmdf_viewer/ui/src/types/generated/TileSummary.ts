// Auto-generated from Rust: src/debug/rmdf_viewer/api.rs::TileSummary, TileBoundsResponse
// Manually maintained to match Rust struct definition

export interface TileBoundsResponse {
  lat_min: number;
  lat_max: number;
  lon_min: number;
  lon_max: number;
}

export interface TileSummary {
  filename: string;
  col: number;
  row: number;
  bounds: TileBoundsResponse;
  size_bytes: number;
  point_count: number;
  line_count: number;
}
