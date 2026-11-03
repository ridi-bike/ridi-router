import * as FileSystem from 'expo-file-system/legacy';
import type { Store } from 'tinybase';

import { RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '../ids';
import { markRmdfManifestTileDownloaded, markRmdfTileRemoteOnly, rowInstalledTileMeta, rowTileMeta } from './rmdfTiles';
import { RMDF_SUPPORTED_FORMAT_VERSION } from '@/rmdf/constants';
import { tileSizeDegrees, writeInstalledManifest } from '@/rmdf/installed';
import { deleteRmdfTiles, ensureRmdfDirectory, getRmdfDirectory, getRmdfTilePath } from '@/rmdf/storage';
import type { RmdfManifestTile, RmdfRelease } from '@/rmdf/types';

function nowIso(): string {
  return new Date().toISOString();
}

/** RMDF format version from the file header (`RMDF` magic, then a little-endian u32). */
async function readTileFormatVersion(filename: string): Promise<number | null> {
  try {
    const base64 = await FileSystem.readAsStringAsync(getRmdfTilePath(filename), {
      encoding: FileSystem.EncodingType.Base64,
      position: 0,
      length: 8,
    });
    const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
    if (bytes.length < 8 || String.fromCharCode(...bytes.slice(0, 4)) !== 'RMDF') return null;
    return bytes[4] | (bytes[5] << 8) | (bytes[6] << 16) | (bytes[7] << 24);
  } catch {
    return null;
  }
}

async function listTileFiles(): Promise<Set<string>> {
  await ensureRmdfDirectory();
  const files = await FileSystem.readDirectoryAsync(getRmdfDirectory());
  return new Set(files.filter((file) => file.endsWith('.rmdf')));
}

function installedFilenames(store: Store): Set<string> {
  const installed = new Set<string>();
  for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
    if (rowInstalledTileMeta(row)) installed.add(typeof row.filename === 'string' ? row.filename : rowId);
  }
  return installed;
}

/**
 * Brings the tile table, the tiles directory and the router manifest back in line:
 * - installed tiles whose file is gone become remote-only;
 * - tile files the table does not know are kept unless their header shows an RMDF format the
 *   router cannot read; `adoptUntrackedTiles` re-registers them once release metadata is loaded,
 *   so a lost or reset local store never costs downloaded data;
 * - with a release, installed tiles from a different tile grid are deleted, because the router
 *   reads one grid size and tile filenames would collide across grids.
 */
export async function reconcileLocalRmdfTiles(store: Store, release: RmdfRelease | null = null): Promise<void> {
  const rmdfFiles = await listTileFiles();
  const checkedAt = nowIso();
  const filesToDelete: string[] = [];

  store.transaction(() => {
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      const filename = typeof row.filename === 'string' ? row.filename : rowId;
      const installed = rowInstalledTileMeta(row);
      if (!installed) {
        if (row.status === 'downloaded') markRmdfTileRemoteOnly(store, rowId, checkedAt);
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

  const installed = installedFilenames(store);
  for (const filename of rmdfFiles) {
    if (installed.has(filename) || filesToDelete.includes(filename)) continue;
    const version = await readTileFormatVersion(filename);
    if (version !== null && version !== RMDF_SUPPORTED_FORMAT_VERSION) filesToDelete.push(filename);
  }
  if (filesToDelete.length > 0) {
    await deleteRmdfTiles(filesToDelete);
    console.info(`[rmdf] removed ${filesToDelete.length} tile file(s) of another grid or RMDF format`);
  }

  await writeInstalledManifest(store);
  store.setValue('rmdf_last_reconciled_at', checkedAt);
}

/** Filenames of tile files on disk that the tile table does not list as installed. */
export async function listUntrackedTileFiles(store: Store): Promise<string[]> {
  const installed = installedFilenames(store);
  return [...(await listTileFiles())].filter((filename) => !installed.has(filename));
}

/**
 * Registers untracked tile files that match a release tile by name and size as installed with that
 * tile's metadata. Size is the strongest check available without hashing in JS; a stale tile of
 * identical size would be taken as current until the next release changes its checksum.
 */
export async function adoptUntrackedTiles(store: Store, tiles: Map<string, RmdfManifestTile>): Promise<number> {
  const adoptedAt = nowIso();
  let adopted = 0;
  for (const filename of await listUntrackedTileFiles(store)) {
    const tile = tiles.get(filename);
    if (!tile) continue;
    const info = await FileSystem.getInfoAsync(getRmdfTilePath(filename));
    if (!info.exists || info.isDirectory || info.size !== tile.size_bytes) continue;
    markRmdfManifestTileDownloaded(store, tile, adoptedAt);
    adopted += 1;
  }
  if (adopted > 0) {
    await writeInstalledManifest(store);
    console.info(`[rmdf] re-registered ${adopted} tile file(s) found on disk`);
  }
  return adopted;
}
