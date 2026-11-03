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
  const chunk = [...release.chunks].sort((a, b) => b.total_bytes - a.total_bytes)[0];
  return [(chunk.bounds.lon_min + chunk.bounds.lon_max) / 2, (chunk.bounds.lat_min + chunk.bounds.lat_max) / 2];
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

export function cellIdAt(longitude: number, latitude: number): string {
  return gridIdAt(longitude, latitude, CELL_SIZE_DEGREES);
}

export function tileIdAt(release: RmdfRelease, longitude: number, latitude: number): string {
  const [col, row] = gridIdAt(longitude, latitude, release.tile_size_degrees).split('_');
  return `tile_${col}_${row}.rmdf`;
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

export type GridBox = {
  id: string;
  level: GridLevel;
  bounds: RmdfBounds;
  totalCount: number;
  totalBytes: number;
  downloadedCount: number;
  outdatedCount: number;
  pendingCount: number;
  failedCount: number;
  selectedCount: number;
  queuePosition?: number;
};

type BoxCounts = Omit<GridBox, 'id' | 'level' | 'bounds' | 'totalCount' | 'totalBytes' | 'queuePosition'>;

function emptyCounts(): BoxCounts {
  return { downloadedCount: 0, outdatedCount: 0, pendingCount: 0, failedCount: 0, selectedCount: 0 };
}

function addStatus(counts: BoxCounts, status: RmdfTileStatus, selected: boolean) {
  if (status === 'downloaded') counts.downloadedCount += 1;
  if (status === 'outdated') counts.outdatedCount += 1;
  if (status === 'queued' || status === 'downloading') counts.pendingCount += 1;
  if (status === 'failed') counts.failedCount += 1;
  if (selected) counts.selectedCount += 1;
}

export type TileStateLookup = {
  rows: Record<string, Row>;
  queuedIds: Set<string>;
  downloadingIds: Set<string>;
  queuePositions: Map<string, number>;
};

/**
 * Chunk boxes come from the release summary, so they need no chunk manifests. Local state is
 * counted from tile rows, which carry the metadata they were selected or installed with.
 */
export function buildChunkBoxes(release: RmdfRelease, loadedTiles: Map<string, RmdfManifestTile>, state: TileStateLookup): GridBox[] {
  const counts = new Map<string, BoxCounts>();
  const countFor = (id: string) => {
    let entry = counts.get(id);
    if (!entry) counts.set(id, (entry = emptyCounts()));
    return entry;
  };
  for (const [filename, row] of Object.entries(state.rows)) {
    const tile = loadedTiles.get(filename);
    const meta = tile ?? parseRowMeta(row);
    if (!meta) continue;
    const status = tile ? tileStatus(tile, row, state.queuedIds, state.downloadingIds) : rowOnlyStatus(row, state);
    if (status === 'remoteOnly' && row.selected !== true) continue;
    addStatus(countFor(chunkIdForTile(meta, release)), status, row.selected === true);
  }
  return release.chunks.map((chunk) => ({
    id: chunk.id,
    level: 'chunk',
    bounds: chunk.bounds,
    totalCount: chunk.tile_count,
    totalBytes: chunk.total_bytes,
    ...(counts.get(chunk.id) ?? emptyCounts()),
  }));
}

function rowOnlyStatus(row: Row, state: TileStateLookup): RmdfTileStatus {
  const filename = String(row.filename ?? '');
  if (state.downloadingIds.has(filename)) return 'downloading';
  if (state.queuedIds.has(filename)) return 'queued';
  if (row.installed_checksum) return row.installed_checksum === row.checksum ? 'downloaded' : 'outdated';
  return row.status === 'failed' ? 'failed' : 'remoteOnly';
}

function parseRowMeta(row: Row): RmdfManifestTile | null {
  const value = row.installed_meta ?? row.meta;
  if (typeof value !== 'string') return null;
  try {
    return JSON.parse(value) as RmdfManifestTile;
  } catch {
    return null;
  }
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
    addStatus(box, tileStatus(tile, row, state.queuedIds, state.downloadingIds), row?.selected === true);
  }
  return [...boxes.values()];
}

export function buildTileBoxes(tiles: RmdfManifestTile[], state: TileStateLookup, viewport: RmdfBounds): GridBox[] {
  const boxes: GridBox[] = [];
  for (const tile of tiles) {
    if (!boundsIntersect(tile.bounds, viewport)) continue;
    const row = state.rows[tile.filename];
    const counts = emptyCounts();
    addStatus(counts, tileStatus(tile, row, state.queuedIds, state.downloadingIds), row?.selected === true);
    boxes.push({
      id: tile.filename,
      level: 'tile',
      bounds: tile.bounds,
      totalCount: 1,
      totalBytes: tile.size_bytes,
      queuePosition: state.queuePositions.get(tile.filename),
      ...counts,
    });
  }
  return boxes;
}

export function tilesInCell(release: RmdfRelease, tiles: Iterable<RmdfManifestTile>, cellId: string): RmdfManifestTile[] {
  const result: RmdfManifestTile[] = [];
  for (const tile of tiles) if (cellIdForTile(tile, release) === cellId) result.push(tile);
  return result;
}

/** Map styling keys off `state`; aggregated boxes report the most actionable state they contain. */
function boxState(box: GridBox): string {
  if (box.selectedCount > 0) return box.selectedCount === box.totalCount ? 'selected' : 'partSelected';
  if (box.pendingCount > 0) return 'pending';
  if (box.failedCount > 0) return 'failed';
  if (box.outdatedCount > 0) return 'outdated';
  if (box.downloadedCount > 0) return box.downloadedCount === box.totalCount ? 'downloaded' : 'partDownloaded';
  return 'remote';
}

function boxLabel(box: GridBox): string {
  if (box.level === 'tile') {
    if (box.queuePosition) return String(box.queuePosition);
    return box.outdatedCount ? 'update' : '';
  }
  const parts = [formatBytes(box.totalBytes)];
  const installed = box.downloadedCount + box.outdatedCount;
  if (installed > 0) parts.push(`${installed}/${box.totalCount} saved`);
  if (box.outdatedCount > 0) parts.push(`${box.outdatedCount} to update`);
  return parts.join('\n');
}

export function boxesToGeoJson(boxes: GridBox[]) {
  return {
    type: 'FeatureCollection' as const,
    features: boxes.map((box) => ({
      type: 'Feature' as const,
      id: box.id,
      properties: { id: box.id, level: box.level, state: boxState(box), label: boxLabel(box) },
      geometry: {
        type: 'Polygon' as const,
        coordinates: [
          [
            [box.bounds.lon_min, box.bounds.lat_min],
            [box.bounds.lon_max, box.bounds.lat_min],
            [box.bounds.lon_max, box.bounds.lat_max],
            [box.bounds.lon_min, box.bounds.lat_max],
            [box.bounds.lon_min, box.bounds.lat_min],
          ],
        ],
      },
    })),
  };
}
