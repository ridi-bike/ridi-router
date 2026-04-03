// Auto-generated from Rust: src/debug/rmdf_viewer/api.rs::LineResponse
// Manually maintained to match Rust struct definition

import type { TagResponse } from './TagResponse';

export interface LineResponse {
  point_a_osm_id: number;
  point_b_osm_id: number;
  point_a: [number, number];  // [lat, lon]
  point_b: [number, number];  // [lat, lon]
  direction: string;  // "BothWays" | "OneWay" | "Roundabout" | "Unknown"
  tags: TagResponse;
}
