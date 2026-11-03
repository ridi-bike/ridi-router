import * as FileSystem from 'expo-file-system/legacy';
import type { Store } from 'tinybase';

import { RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '../ids';
import { markRmdfTileRemoteOnly, rowInstalledTileMeta, rowTileMeta } from './rmdfTiles';
import { tileSizeDegrees, writeInstalledManifest } from '@/rmdf/installed';
import { deleteRmdfTiles, ensureRmdfDirectory, getRmdfDirectory } from '@/rmdf/storage';
import type { RmdfRelease } from '@/rmdf/types';

function nowIso(): string {
  return new Date().toISOString();
}

/**
 * Brings the tile table, the tiles directory and the router manifest back in line:
 * - installed tiles whose file is gone become remote-only;
 * - tile files the table does not know as installed (e.g. RMDF v1 tiles from before releases)
 *   are deleted, since the router can only read tiles of the supported format;
 * - with a release, installed tiles from a different tile grid are deleted, because the router
 *   reads one grid size and tile filenames would collide across grids.
 */
export async function reconcileLocalRmdfTiles(store: Store, release: RmdfRelease | null = null): Promise<void> {
  await ensureRmdfDirectory();
  const files = await FileSystem.readDirectoryAsync(getRmdfDirectory());
  const rmdfFiles = new Set(files.filter((file) => file.endsWith('.rmdf')));
  const checkedAt = nowIso();
  const filesToDelete: string[] = [];
  const installedFiles = new Set<string>();

  store.transaction(() => {
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      const filename = typeof row.filename === 'string' ? row.filename : rowId;
      const installed = rowInstalledTileMeta(row);
      if (!installed) {
        if (row.status === 'downloaded') {
          // Downloaded before tiles carried release metadata.
          filesToDelete.push(filename);
          markRmdfTileRemoteOnly(store, rowId, checkedAt);
        }
        continue;
      }
      if (!rmdfFiles.has(filename)) {
        markRmdfTileRemoteOnly(store, rowId, checkedAt);
        continue;
      }
      if (release && tileSizeDegrees(installed) !== release.tile_size_degrees) {
        filesToDelete.push(filename);
        store.delRow(RMDF_TILES_TABLE, rowId);
        continue;
      }
      installedFiles.add(filename);
      store.setCell(RMDF_TILES_TABLE, rowId, 'last_checked_at', checkedAt);
    }

    for (const [rowId, row] of Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE))) {
      const filename = String(row.filename ?? row.tile_id ?? '');
      if (!rowTileMeta(store.getRow(RMDF_TILES_TABLE, filename))) store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, rowId);
    }
    const queued = new Set(Object.values(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE)).map((row) => String(row.filename ?? row.tile_id ?? '')));
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      if ((row.status === 'queued' || row.status === 'downloading') && !queued.has(rowId)) {
        store.setCell(RMDF_TILES_TABLE, rowId, 'status', row.installed_meta ? 'downloaded' : 'remoteOnly');
      }
    }
  });

  for (const filename of rmdfFiles) {
    if (!installedFiles.has(filename)) filesToDelete.push(filename);
  }
  if (filesToDelete.length > 0) {
    const unique = [...new Set(filesToDelete)].filter((filename) => !installedFiles.has(filename));
    await deleteRmdfTiles(unique);
    console.info(`[rmdf] removed ${unique.length} tile file(s) not installed from a release`);
  }

  await writeInstalledManifest(store);
  store.setValue('rmdf_last_reconciled_at', checkedAt);
}
