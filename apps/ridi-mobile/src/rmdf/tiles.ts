import type { Row } from 'tinybase';

import type { RmdfBounds, RmdfChunkSummary, RmdfManifestTile, RmdfRelease, RmdfTileStatus } from './types';

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 MB';
  const gb = bytes / 1024 / 1024 / 1024;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = bytes / 1024 / 1024;
  return `${mb.toFixed(1)} MB`;
}

export function hitTestTile(tiles: RmdfManifestTile[], longitude: number, latitude: number): RmdfManifestTile | null {
  return (
    tiles.find(
      (tile) =>
        longitude >= tile.bounds.lon_min &&
        longitude <= tile.bounds.lon_max &&
        latitude >= tile.bounds.lat_min &&
        latitude <= tile.bounds.lat_max
    ) ?? null
  );
}

export function releaseCenter(release: RmdfRelease): [number, number] {
  if (release.chunks.length === 0) return [24.1, 56.9];
  // Chunk bounds are whole grid cells; the biggest chunk is a reasonable place to start.
  const bounds = chunkDrawBounds([...release.chunks].sort((a, b) => b.total_bytes - a.total_bytes)[0]);
  return [(bounds.lon_min + bounds.lon_max) / 2, (bounds.lat_min + bounds.lat_max) / 2];
}

/** The area a chunk square covers on the map: its tiles' extent, not the whole grid cell. */
export function chunkDrawBounds(chunk: RmdfChunkSummary): RmdfBounds {
  return chunk.tile_bounds ?? chunk.bounds;
}

/**
 * What one tap selects at the current zoom: a whole 10 deg chunk when zoomed out, a 1 deg cell at
 * mid zoom, a single tile when zoomed in. The cell level is skipped when tiles are 1 deg or larger.
 */
export type GridLevel = 'chunk' | 'cell' | 'tile';

export const CELL_SIZE_DEGREES = 1;
const CHUNK_MAX_ZOOM = 5.5;
const CELL_MAX_ZOOM = 8.5;

export function gridLevelForZoom(zoom: number, release: RmdfRelease): GridLevel {
  if (zoom < CHUNK_MAX_ZOOM) return 'chunk';
  if (zoom < CELL_MAX_ZOOM && release.tile_size_degrees < CELL_SIZE_DEGREES) return 'cell';
  return 'tile';
}

function tilesPer(sizeDegrees: number, release: RmdfRelease): number {
  return Math.max(1, Math.round(sizeDegrees / release.tile_size_degrees));
}

export function chunkIdForTile(tile: Pick<RmdfManifestTile, 'col' | 'row'>, release: RmdfRelease): string {
  const per = tilesPer(release.chunk_size_degrees, release);
  return `${Math.floor(tile.col / per)}_${Math.floor(tile.row / per)}`;
}

export function cellIdForTile(tile: Pick<RmdfManifestTile, 'col' | 'row'>, release: RmdfRelease): string {
  const per = tilesPer(CELL_SIZE_DEGREES, release);
  return `${Math.floor(tile.col / per)}_${Math.floor(tile.row / per)}`;
}

function gridIdAt(longitude: number, latitude: number, sizeDegrees: number): string {
  return `${Math.floor((longitude + 180) / sizeDegrees)}_${Math.floor((latitude + 90) / sizeDegrees)}`;
}

function gridBounds(id: string, sizeDegrees: number): RmdfBounds {
  const [x, y] = id.split('_').map(Number);
  return {
    lon_min: x * sizeDegrees - 180,
    lon_max: (x + 1) * sizeDegrees - 180,
    lat_min: y * sizeDegrees - 90,
    lat_max: (y + 1) * sizeDegrees - 90,
  };
}

export function chunkAt(release: RmdfRelease, longitude: number, latitude: number): RmdfChunkSummary | null {
  const id = gridIdAt(longitude, latitude, release.chunk_size_degrees);
  return release.chunks.find((chunk) => chunk.id === id) ?? null;
}

export function boundsContain(bounds: RmdfBounds, longitude: number, latitude: number): boolean {
  return longitude >= bounds.lon_min && longitude <= bounds.lon_max && latitude >= bounds.lat_min && latitude <= bounds.lat_max;
}

export function cellIdAt(longitude: number, latitude: number): string {
  return gridIdAt(longitude, latitude, CELL_SIZE_DEGREES);
}

export function tilePositionFromFilename(filename: string): { col: number; row: number } | null {
  const match = /^tile_(\d+)_(\d+)\.rmdf$/.exec(filename);
  return match ? { col: Number(match[1]), row: Number(match[2]) } : null;
}

/** Map longitudes can run past ±180 after panning across the antimeridian. */
export function normalizeLongitude(longitude: number): number {
  return ((((longitude + 180) % 360) + 360) % 360) - 180;
}

/** A west/east span in -180..180; a span that wraps or covers the globe becomes the whole range. */
export function normalizeLongitudeRange(west: number, east: number): Pick<RmdfBounds, 'lon_min' | 'lon_max'> {
  if (east - west >= 360) return { lon_min: -180, lon_max: 180 };
  const lonMin = normalizeLongitude(west);
  const lonMax = normalizeLongitude(east);
  return lonMin <= lonMax ? { lon_min: lonMin, lon_max: lonMax } : { lon_min: -180, lon_max: 180 };
}

export function boundsIntersect(a: RmdfBounds, b: RmdfBounds): boolean {
  return a.lon_min < b.lon_max && a.lon_max > b.lon_min && a.lat_min < b.lat_max && a.lat_max > b.lat_min;
}

/** Tile state as shown on the map, from the release tile and its local row. */
export function tileStatus(tile: RmdfManifestTile, row: Row | undefined, queuedIds: Set<string>, downloadingIds: Set<string>): RmdfTileStatus {
  if (downloadingIds.has(tile.filename)) return 'downloading';
  if (queuedIds.has(tile.filename)) return 'queued';
  const installed = row?.installed_checksum;
  if (installed) return installed === tile.checksum ? 'downloaded' : 'outdated';
  if (row?.status === 'failed') return 'failed';
  return 'remoteOnly';
}

/** One tappable grid square; it only reports how much of it is selected and on the device. */
export type GridBox = {
  id: string;
  level: GridLevel;
  bounds: RmdfBounds;
  totalCount: number;
  totalBytes: number;
  selectedCount: number;
  installedCount: number;
  installedBytes: number;
};

type BoxCounts = Pick<GridBox, 'selectedCount' | 'installedCount' | 'installedBytes'>;

function emptyCounts(): BoxCounts {
  return { selectedCount: 0, installedCount: 0, installedBytes: 0 };
}

function addTile(counts: BoxCounts, row: Row | undefined, installedBytes: number | null) {
  if (row?.selected === true) counts.selectedCount += 1;
  if (installedBytes !== null) {
    counts.installedCount += 1;
    counts.installedBytes += installedBytes;
  }
}

export type TileStateLookup = {
  rows: Record<string, Row>;
  queuedIds: Set<string>;
  downloadingIds: Set<string>;
};

function parseRowMeta(row: Row): RmdfManifestTile | null {
  const value = row.installed_meta ?? row.meta;
  if (typeof value !== 'string') return null;
  try {
    return JSON.parse(value) as RmdfManifestTile;
  } catch {
    return null;
  }
}

function installedSize(row: Row | undefined): number | null {
  if (!row?.installed_meta) return null;
  return parseRowMeta(row)?.size_bytes ?? 0;
}

/**
 * Chunk boxes come from the release summary, so they need no chunk manifests. Local state is
 * counted from tile rows, which carry the metadata they were selected or installed with.
 */
export function buildChunkBoxes(release: RmdfRelease, state: TileStateLookup): GridBox[] {
  const counts = new Map<string, BoxCounts>();
  for (const row of Object.values(state.rows)) {
    const meta = parseRowMeta(row);
    const installed = installedSize(row);
    if (!meta || (installed === null && row.selected !== true)) continue;
    const id = chunkIdForTile(meta, release);
    let entry = counts.get(id);
    if (!entry) counts.set(id, (entry = emptyCounts()));
    addTile(entry, row, installed);
  }
  return release.chunks.map((chunk) => ({
    id: chunk.id,
    level: 'chunk',
    bounds: chunkDrawBounds(chunk),
    totalCount: chunk.tile_count,
    totalBytes: chunk.total_bytes,
    ...(counts.get(chunk.id) ?? emptyCounts()),
  }));
}

export function buildCellBoxes(release: RmdfRelease, tiles: RmdfManifestTile[], state: TileStateLookup, viewport: RmdfBounds): GridBox[] {
  const boxes = new Map<string, GridBox>();
  for (const tile of tiles) {
    const id = cellIdForTile(tile, release);
    let box = boxes.get(id);
    if (!box) {
      const bounds = gridBounds(id, CELL_SIZE_DEGREES);
      if (!boundsIntersect(bounds, viewport)) continue;
      box = { id, level: 'cell', bounds, totalCount: 0, totalBytes: 0, ...emptyCounts() };
      boxes.set(id, box);
    }
    box.totalCount += 1;
    box.totalBytes += tile.size_bytes;
    const row = state.rows[tile.filename];
    addTile(box, row, installedSize(row));
  }
  return [...boxes.values()];
}

export function buildTileBoxes(tiles: RmdfManifestTile[], state: TileStateLookup, viewport: RmdfBounds): GridBox[] {
  const boxes: GridBox[] = [];
  for (const tile of tiles) {
    if (!boundsIntersect(tile.bounds, viewport)) continue;
    const row = state.rows[tile.filename];
    const box: GridBox = { id: tile.filename, level: 'tile', bounds: tile.bounds, totalCount: 1, totalBytes: tile.size_bytes, ...emptyCounts() };
    addTile(box, row, installedSize(row));
    boxes.push(box);
  }
  return boxes;
}

export function tilesInCell(release: RmdfRelease, tiles: Iterable<RmdfManifestTile>, cellId: string): RmdfManifestTile[] {
  const result: RmdfManifestTile[] = [];
  for (const tile of tiles) if (cellIdForTile(tile, release) === cellId) result.push(tile);
  return result;
}

function boxSelection(box: GridBox): 'all' | 'some' | 'none' {
  if (box.selectedCount === 0) return 'none';
  return box.selectedCount >= box.totalCount ? 'all' : 'some';
}

/** What is still to download in the square; single tiles are too small for a label. */
function boxLabel(box: GridBox): string {
  if (box.level === 'tile') return '';
  if (box.installedCount >= box.totalCount) return 'Downloaded';
  const remaining = formatBytes(box.totalBytes - box.installedBytes);
  return box.installedCount > 0 ? `${remaining} left` : remaining;
}

function polygon(bounds: RmdfBounds) {
  return {
    type: 'Polygon' as const,
    coordinates: [
      [
        [bounds.lon_min, bounds.lat_min],
        [bounds.lon_max, bounds.lat_min],
        [bounds.lon_max, bounds.lat_max],
        [bounds.lon_min, bounds.lat_max],
        [bounds.lon_min, bounds.lat_min],
      ],
    ],
  };
}

export function boxesToGeoJson(boxes: GridBox[]) {
  return {
    type: 'FeatureCollection' as const,
    features: boxes.map((box) => ({
      type: 'Feature' as const,
      id: box.id,
      properties: { id: box.id, level: box.level, selection: boxSelection(box) },
      geometry: polygon(box.bounds),
    })),
  };
}

/** One label per square at its center; polygon labels repeat across render tiles. */
export function boxLabelsToGeoJson(boxes: GridBox[]) {
  return {
    type: 'FeatureCollection' as const,
    features: boxes.flatMap((box) => {
      const label = boxLabel(box);
      if (!label) return [];
      const center = [(box.bounds.lon_min + box.bounds.lon_max) / 2, (box.bounds.lat_min + box.bounds.lat_max) / 2];
      return [{ type: 'Feature' as const, id: box.id, properties: { label }, geometry: { type: 'Point' as const, coordinates: center } }];
    }),
  };
}

export type TileFillState = 'downloaded' | 'outdated' | 'pending' | 'failed' | 'selected';

/**
 * The tiles themselves, at every zoom: what is on the device (or on its way) and what is selected
 * to download. Downloaded tiles stay green when selected so the saved area is always visible.
 */
export function tileStatesToGeoJson(state: TileStateLookup, loadedTiles: Map<string, RmdfManifestTile>) {
  const features = [];
  for (const [filename, row] of Object.entries(state.rows)) {
    const release = loadedTiles.get(filename);
    const meta = release ?? parseRowMeta(row);
    if (!meta) continue;
    let fill: TileFillState | null = null;
    if (state.downloadingIds.has(filename) || state.queuedIds.has(filename)) fill = 'pending';
    else if (row.installed_checksum) fill = row.installed_checksum === (release?.checksum ?? row.checksum) ? 'downloaded' : 'outdated';
    else if (row.selected === true) fill = 'selected';
    else if (row.status === 'failed') fill = 'failed';
    if (!fill) continue;
    features.push({ type: 'Feature' as const, id: filename, properties: { fill }, geometry: polygon(meta.bounds) });
  }
  return { type: 'FeatureCollection' as const, features };
}
