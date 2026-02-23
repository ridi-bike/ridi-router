// Auto-generated from Rust: src/debug/rmdf_viewer/api.rs::ManifestResponse
// Manually maintained to match Rust struct definition

import type { TileSummary } from './TileSummary';

export interface ManifestResponse {
  version: string;
  tile_size_degrees: number;
  format_version: number;
  generated_at: string;
  tiles: TileSummary[];
}
