import * as FileSystem from 'expo-file-system/legacy';
import type { Store } from 'tinybase';

import { RMDF_SUPPORTED_FORMAT_VERSION } from './constants';
import { ensureRmdfDirectory, getRmdfManifestPath } from './storage';
import type { RmdfInstalledManifest, RmdfManifestTile } from './types';
import { RMDF_TILES_TABLE } from '@/tinybase/ids';
import { rowInstalledTileMeta } from '@/tinybase/local/rmdfTiles';

export function getInstalledTiles(store: Store): RmdfManifestTile[] {
  const tiles: RmdfManifestTile[] = [];
  for (const row of Object.values(store.getTable(RMDF_TILES_TABLE))) {
    const meta = rowInstalledTileMeta(row);
    if (meta) tiles.push(meta);
  }
  return tiles;
}

/** Grid size a tile was generated with, recovered from its grid position (origin -180/-90). */
export function tileSizeDegrees(tile: RmdfManifestTile): number {
  const size = tile.col > 0 ? (tile.bounds.lon_min + 180) / tile.col : (tile.bounds.lat_min + 90) / Math.max(tile.row, 1);
  return Math.round(size * 10000) / 10000;
}

export function buildInstalledManifest(tiles: RmdfManifestTile[]): RmdfInstalledManifest {
  return {
    version: 'ridi-mobile',
    tile_size_degrees: tiles.length ? tileSizeDegrees(tiles[0]) : 0.1,
    format_version: RMDF_SUPPORTED_FORMAT_VERSION,
    generated_at: new Date().toISOString(),
    source_files: [],
    tiles: tiles.map(({ path: _path, ...tile }) => ({ ...tile, neighbors: {} })),
  };
}

let pendingWrite: Promise<void> = Promise.resolve();
let scheduled: ReturnType<typeof setTimeout> | null = null;

/** Rewrites the router manifest now (queued behind any write in progress). */
export function writeInstalledManifest(store: Store): Promise<void> {
  if (scheduled) {
    clearTimeout(scheduled);
    scheduled = null;
  }
  pendingWrite = pendingWrite.catch(() => undefined).then(async () => {
    await ensureRmdfDirectory();
    const path = getRmdfManifestPath();
    const temp = `${path}.tmp`;
    await FileSystem.writeAsStringAsync(temp, JSON.stringify(buildInstalledManifest(getInstalledTiles(store))));
    await FileSystem.deleteAsync(path, { idempotent: true });
    await FileSystem.moveAsync({ from: temp, to: path });
  });
  return pendingWrite;
}

/** Coalesces bursts of changes (one per downloaded tile) into one write. */
export function scheduleInstalledManifestWrite(store: Store, delayMs = 1000) {
  if (scheduled) clearTimeout(scheduled);
  scheduled = setTimeout(() => {
    scheduled = null;
    void writeInstalledManifest(store);
  }, delayMs);
}
