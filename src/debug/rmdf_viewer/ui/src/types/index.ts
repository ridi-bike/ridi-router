// Re-export all types from generated files
export type { ManifestResponse } from './generated/ManifestResponse';

export type {
  TileSummary,
  TileBoundsResponse,
} from './generated/TileSummary';

export type {
  TileResponse,
  TileHeader,
} from './generated/TileResponse';

export type { PointResponse } from './generated/PointResponse';
export type { LineResponse } from './generated/LineResponse';
export type { TagResponse } from './generated/TagResponse';

export interface MilitaryGeoJsonFeatureCollection {
  type: 'FeatureCollection';
  features: MilitaryGeoJsonFeature[];
}

export interface MilitaryGeoJsonFeature {
  type: 'Feature';
  geometry: MilitaryGeoJsonGeometry;
  properties?: {
    polygon_index?: number;
    source?: string;
  };
}

export interface MilitaryGeoJsonGeometry {
  type: 'MultiPolygon';
  coordinates: number[][][][];
}

