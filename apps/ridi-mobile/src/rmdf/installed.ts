import * as FileSystem from 'expo-file-system/legacy';
import type { Store } from 'tinybase';

import { RMDF_SUPPORTED_FORMAT_VERSION } from './constants';
import { ensureRmdfDirectory, getRmdfManifestPath, replaceFile } from './storage';
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

/**
 * Grid size a tile was generated with, from its extent. Only a fallback for stores written before
 * the release tile size was recorded; release tiles are exactly one grid cell wide.
 */
export function tileSizeDegrees(tile: RmdfManifestTile): number {
  return Math.round((tile.bounds.lon_max - tile.bounds.lon_min) * 10000) / 10000;
}

export function buildInstalledManifest(tiles: RmdfManifestTile[], tileSizeDegreesValue: number): RmdfInstalledManifest {
  return {
    version: 'ridi-mobile',
    tile_size_degrees: tileSizeDegreesValue,
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
    const tiles = getInstalledTiles(store);
    const stored = store.getValue('rmdf_tile_size_degrees');
    const size = typeof stored === 'number' && stored > 0 ? stored : tiles.length ? tileSizeDegrees(tiles[0]) : 0.1;
    await FileSystem.writeAsStringAsync(temp, JSON.stringify(buildInstalledManifest(tiles, size)));
    await replaceFile(temp, path);
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
