export type RmdfBounds = {
  lat_min: number;
  lat_max: number;
  lon_min: number;
  lon_max: number;
};

/** `index.json`: the current release per RMDF format version. */
export type RmdfTileIndex = {
  index_version: number;
  releases: RmdfRelease[];
};

export type RmdfRelease = {
  format_version: number;
  release_id: string;
  published_at: string;
  tile_size_degrees: number;
  chunk_size_degrees: number;
  tile_count: number;
  total_bytes: number;
  source_files: string[];
  chunks: RmdfChunkSummary[];
};

export type RmdfChunkSummary = {
  id: string;
  /** Grid cell of the chunk. */
  bounds: RmdfBounds;
  /** Extent of the chunk's tiles; the selectable area drawn on the map. */
  tile_bounds?: RmdfBounds;
  tile_count: number;
  total_bytes: number;
  /** Chunk manifest path relative to the base URL. */
  manifest: string;
};

export type RmdfChunkManifest = {
  format_version: number;
  release_id: string;
  tile_size_degrees: number;
  tiles: RmdfManifestTile[];
};

export type RmdfManifestTile = {
  filename: string;
  col: number;
  row: number;
  bounds: RmdfBounds;
  size_bytes: number;
  point_count: number;
  line_count: number;
  /** Content checksum; doubles as the tile version. */
  checksum: string;
  /** Tile contents path relative to the base URL. */
  path: string;
};

/** Router manifest (`manifest.json` in the tiles directory), written from installed tiles. */
export type RmdfInstalledManifest = {
  version: string;
  tile_size_degrees: number;
  format_version: number;
  generated_at: string;
  source_files: string[];
  tiles: (Omit<RmdfManifestTile, 'path'> & { neighbors: Record<string, string | null> })[];
};

export type RmdfTileStatus = 'remoteOnly' | 'downloaded' | 'outdated' | 'queued' | 'downloading' | 'failed';

export type RmdfReleaseLoadResult = {
  release: RmdfRelease;
  source: 'remote' | 'cache';
  /** Downloads are disabled when the release comes from the offline cache. */
  deleteOnly: boolean;
  warning?: string;
};
