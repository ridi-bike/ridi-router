// Auto-generated from Rust: src/debug/rmdf_viewer/api.rs::TileResponse, TileHeader
// Manually maintained to match Rust struct definition

import type { PointResponse } from './PointResponse';
import type { LineResponse } from './LineResponse';

export interface TileHeader {
  point_count: number;
  line_count: number;
}

export interface TileResponse {
  filename: string;
  header: TileHeader;
  points: PointResponse[];
  lines: LineResponse[];
}
