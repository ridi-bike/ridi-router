import * as FileSystem from 'expo-file-system/legacy';
import type { Row, Store } from 'tinybase';

import { RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE, getRmdfDownloadQueueRowId } from '../ids';
import { markRmdfManifestTileDownloaded, markRmdfTileRemoteOnly, rowInstalledTileMeta, rowTileMeta } from './rmdfTiles';
import { RMDF_SUPPORTED_FORMAT_VERSION } from '@/rmdf/constants';
import { tileSizeDegrees, writeInstalledManifest } from '@/rmdf/installed';
import { deleteRmdfTiles, ensureRmdfDirectory, getRmdfDirectory, getRmdfTilePath, isTileFile, readTileFormatVersion } from '@/rmdf/storage';
import type { RmdfManifestTile, RmdfRelease } from '@/rmdf/types';

function nowIso(): string {
  return new Date().toISOString();
}

async function listTileFiles(): Promise<Set<string>> {
  await ensureRmdfDirectory();
  const files = await FileSystem.readDirectoryAsync(getRmdfDirectory());
  return new Set(files.filter((file) => file.endsWith('.rmdf')));
}

function rowFilename(rowId: string, row: Row): string {
  return typeof row.filename === 'string' ? row.filename : rowId;
}

function installedFilenames(store: Store): Set<string> {
  const installed = new Set<string>();
  for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
    if (rowInstalledTileMeta(row)) installed.add(rowFilename(rowId, row));
  }
  return installed;
}

function dropTile(store: Store, filename: string) {
  store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, getRmdfDownloadQueueRowId(filename));
  markRmdfTileRemoteOnly(store, filename);
}

/**
 * Brings the tile table, the tiles directory and the router manifest back in line. It never
 * deletes a file it cannot prove unusable, so a lost or reset local store costs no data:
 * - installed tiles whose file is gone are dropped, unless they were installed after the
 *   directory was listed (a download finishing while this runs);
 * - files the table does not know are kept unless their header is another RMDF format;
 *   `adoptUntrackedTiles` re-registers them once release metadata is loaded;
 * - when the supported RMDF format changes, installed tiles are header-checked once and tiles of
 *   another format are deleted;
 * - with a release: its tile size is recorded for the router manifest, and installed, queued or
 *   selected tiles of another grid are dropped (the router reads one grid and filenames collide).
 */
export async function reconcileLocalRmdfTiles(store: Store, release: RmdfRelease | null = null): Promise<void> {
  const listedAt = nowIso();
  const rmdfFiles = await listTileFiles();
  const filesToDelete = new Set<string>();

  const formatChecked = store.getValue('rmdf_checked_format_version') === RMDF_SUPPORTED_FORMAT_VERSION;
  const wrongFormat = new Set<string>();
  if (!formatChecked) {
    for (const filename of installedFilenames(store)) {
      const version = await readTileFormatVersion(getRmdfTilePath(filename));
      if (version !== null && version !== RMDF_SUPPORTED_FORMAT_VERSION) wrongFormat.add(filename);
    }
  }

  store.transaction(() => {
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      const filename = rowFilename(rowId, row);
      const installed = rowInstalledTileMeta(row);
      const meta = installed ?? rowTileMeta(row);
      if (release && meta && tileSizeDegrees(meta) !== release.tile_size_degrees) {
        if (installed) filesToDelete.add(filename);
        dropTile(store, rowId);
        continue;
      }
      if (!installed) {
        if (row.status === 'downloaded') dropTile(store, rowId);
        continue;
      }
      if (wrongFormat.has(filename)) {
        filesToDelete.add(filename);
        dropTile(store, rowId);
        continue;
      }
      const installedDuringListing = typeof row.downloaded_at === 'string' && row.downloaded_at >= listedAt;
      if (!rmdfFiles.has(filename) && !installedDuringListing) {
        dropTile(store, rowId);
        continue;
      }
      store.setCell(RMDF_TILES_TABLE, rowId, 'last_checked_at', listedAt);
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

    if (release) store.setValue('rmdf_tile_size_degrees', release.tile_size_degrees);
    store.setValue('rmdf_checked_format_version', RMDF_SUPPORTED_FORMAT_VERSION);
    store.setValue('rmdf_last_reconciled_at', listedAt);
  });

  const installed = installedFilenames(store);
  for (const filename of rmdfFiles) {
    if (installed.has(filename) || filesToDelete.has(filename)) continue;
    const version = await readTileFormatVersion(getRmdfTilePath(filename));
    if (version !== null && version !== RMDF_SUPPORTED_FORMAT_VERSION) filesToDelete.add(filename);
  }

  // Rows are settled first so the router manifest never lists a tile that is about to go.
  await writeInstalledManifest(store);
  if (filesToDelete.size > 0) {
    await deleteRmdfTiles([...filesToDelete]);
    console.info(`[rmdf] removed ${filesToDelete.size} tile file(s) of another grid or RMDF format`);
  }
}

/** Filenames of tile files on disk that the tile table does not list as installed. */
export async function listUntrackedTileFiles(store: Store): Promise<string[]> {
  const installed = installedFilenames(store);
  return [...(await listTileFiles())].filter((filename) => !installed.has(filename));
}

/** Registers untracked tile files that are byte-for-byte a release tile (size and checksum). */
export async function adoptUntrackedTiles(store: Store, tiles: Map<string, RmdfManifestTile>): Promise<number> {
  const adoptedAt = nowIso();
  let adopted = 0;
  for (const filename of await listUntrackedTileFiles(store)) {
    const tile = tiles.get(filename);
    if (!tile || !(await isTileFile(getRmdfTilePath(filename), tile))) continue;
    markRmdfManifestTileDownloaded(store, tile, adoptedAt);
    adopted += 1;
  }
  if (adopted > 0) {
    await writeInstalledManifest(store);
    console.info(`[rmdf] re-registered ${adopted} tile file(s) found on disk`);
  }
  return adopted;
}

/**
 * Brings selected and queued tiles to the current release: their metadata (and so the object they
 * download) is replaced with the release's, and tiles the release no longer has are dropped.
 * `releaseTiles` must hold every release chunk containing a tile the store knows.
 */
export function refreshPendingTiles(store: Store, releaseTiles: Map<string, RmdfManifestTile>) {
  store.transaction(() => {
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      const meta = rowTileMeta(row);
      if (!meta || row.status === 'downloading') continue;
      const current = releaseTiles.get(rowFilename(rowId, row));
      if (!current) {
        if (!row.installed_meta) dropTile(store, rowId);
        continue;
      }
      if (current.checksum === meta.checksum) continue;
      store.setPartialRow(RMDF_TILES_TABLE, rowId, { meta: JSON.stringify(current), checksum: current.checksum, size_bytes: current.size_bytes });
      const queueRowId = getRmdfDownloadQueueRowId(rowId);
      if (!store.hasRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId)) continue;
      if (row.installed_checksum === current.checksum) {
        // The update it was queued for is no longer needed.
        store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
        store.setCell(RMDF_TILES_TABLE, rowId, 'status', 'downloaded');
      } else {
        store.setCell(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, 'expected_bytes', current.size_bytes);
      }
    }
  });
}
