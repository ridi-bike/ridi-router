export type RmdfManifest = {
  version: string;
  tile_size_degrees: number;
  format_version: number;
  generated_at: string;
  source_files: string[];
  tiles: RmdfManifestTile[];
};

export type RmdfManifestTile = {
  filename: string;
  col: number;
  row: number;
  bounds: {
    lat_min: number;
    lat_max: number;
    lon_min: number;
    lon_max: number;
  };
  neighbors: Record<string, string | null>;
  size_bytes: number;
  point_count: number;
  line_count: number;
  checksum: string;
};

export type RmdfTileStatus = 'remoteOnly' | 'downloaded' | 'queued' | 'downloading' | 'failed';

export type RmdfTileViewModel = {
  id: string;
  manifestTile: RmdfManifestTile;
  status: RmdfTileStatus;
  selected: boolean;
  localPath: string;
  queuePosition?: number;
};

export type RmdfGeoJsonFeature = {
  type: 'Feature';
  id: string;
  properties: {
    id: string;
    filename: string;
    status: RmdfTileStatus;
    selected: boolean;
    queuePosition: number;
    label: string;
  };
  geometry: {
    type: 'Polygon';
    coordinates: number[][][];
  };
};

export type RmdfGeoJsonFeatureCollection = {
  type: 'FeatureCollection';
  features: RmdfGeoJsonFeature[];
};

export type RmdfManifestLoadResult =
  | { manifest: RmdfManifest; source: 'remote'; deleteOnly: false; warning?: string }
  | { manifest: RmdfManifest; source: 'cache'; deleteOnly: true; warning: string };
