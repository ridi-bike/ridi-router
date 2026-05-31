import * as FileSystem from 'expo-file-system/legacy';
import type { Store } from 'tinybase';

import { RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '../ids';
import { markRmdfManifestTileDownloaded, queueRmdfManifestTile } from './rmdfTiles';
import { readCachedManifest } from '@/rmdf/manifest';
import { ensureRmdfDirectory, getRmdfDirectory, getRmdfTilePath, readTextIfExists, getRmdfQueuePath, writeText } from '@/rmdf/storage';
import type { RmdfManifest, RmdfManifestTile } from '@/rmdf/types';

type LegacyQueueItem = { tile?: RmdfManifestTile };

function nowIso(): string {
  return new Date().toISOString();
}

function isComplete(info: FileSystem.FileInfo, expectedBytes?: number): boolean {
  if (!info.exists || info.isDirectory) return false;
  return expectedBytes == null || info.size === expectedBytes;
}

function manifestByFilename(manifest: RmdfManifest | null): Map<string, RmdfManifestTile> {
  return new Map((manifest?.tiles ?? []).map((tile) => [tile.filename, tile]));
}

export async function migrateLegacyRmdfQueueToLocalStore(store: Store): Promise<void> {
  const text = await readTextIfExists(getRmdfQueuePath());
  if (!text) return;

  let parsed: LegacyQueueItem[];
  try {
    parsed = JSON.parse(text) as LegacyQueueItem[];
  } catch {
    await writeText(getRmdfQueuePath(), '[]');
    return;
  }

  const queued = Array.isArray(parsed) ? parsed.map((item) => item.tile).filter((tile): tile is RmdfManifestTile => Boolean(tile?.filename)) : [];
  if (queued.length === 0) {
    await writeText(getRmdfQueuePath(), '[]');
    return;
  }

  const queuedAt = nowIso();
  store.transaction(() => {
    queued.forEach((tile, index) => queueRmdfManifestTile(store, tile, index, queuedAt));
  });

  await writeText(getRmdfQueuePath(), '[]');
}

export async function reconcileLocalRmdfTiles(store: Store, manifest: RmdfManifest | null = null): Promise<void> {
  await ensureRmdfDirectory();
  const effectiveManifest = manifest ?? (await readCachedManifest().catch(() => null));
  const manifestTiles = manifestByFilename(effectiveManifest);
  const files = await FileSystem.readDirectoryAsync(getRmdfDirectory());
  const rmdfFiles = new Set(files.filter((file) => file.endsWith('.rmdf')));
  const checkedAt = nowIso();

  store.transaction(() => {
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      if (row.status !== 'downloaded') continue;
      const filename = typeof row.filename === 'string' ? row.filename : rowId;
      const tile = manifestTiles.get(filename);
      if (!rmdfFiles.has(filename)) {
        store.delRow(RMDF_TILES_TABLE, rowId);
        continue;
      }
      store.setCell(RMDF_TILES_TABLE, rowId, 'last_checked_at', checkedAt);
      if (tile && row.size_bytes !== tile.size_bytes) {
        store.setCell(RMDF_TILES_TABLE, rowId, 'size_bytes', tile.size_bytes);
      }
    }
  });

  for (const filename of rmdfFiles) {
    const tile = manifestTiles.get(filename);
    if (!tile || store.hasRow(RMDF_TILES_TABLE, filename)) continue;
    const info = await FileSystem.getInfoAsync(getRmdfTilePath(filename));
    if (!isComplete(info, tile.size_bytes)) continue;
    markRmdfManifestTileDownloaded(store, tile, checkedAt);
    store.setCell(RMDF_TILES_TABLE, filename, 'selected', false);
  }

  for (const [rowId, row] of Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE))) {
    const filename = typeof row.filename === 'string' ? row.filename : typeof row.tile_id === 'string' ? row.tile_id : '';
    if (!filename) continue;
    const tile = manifestTiles.get(filename);
    if (!tile) continue;
    const info = await FileSystem.getInfoAsync(getRmdfTilePath(filename));
    if (isComplete(info, tile.size_bytes)) {
      store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, rowId);
      markRmdfManifestTileDownloaded(store, tile, checkedAt);
    }
  }

  store.setValue('rmdf_last_reconciled_at', checkedAt);
}
