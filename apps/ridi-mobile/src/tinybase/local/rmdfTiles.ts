import type { Row, Store } from 'tinybase';

import type { RmdfTileStatus } from './schema';
import { getRmdfDownloadQueueRowId, RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '../ids';
import { getRmdfTempTilePath, getRmdfTilePath } from '@/rmdf/storage';
import type { RmdfManifestTile } from '@/rmdf/types';

export function parseTileMeta(value: unknown): RmdfManifestTile | null {
  if (typeof value !== 'string' || !value) return null;
  try {
    return JSON.parse(value) as RmdfManifestTile;
  } catch {
    return null;
  }
}

/** Release metadata of the tile as last selected or queued. */
export function rowTileMeta(row: Row): RmdfManifestTile | null {
  return parseTileMeta(row.meta);
}

/** Metadata of the tile file currently on disk, if any. */
export function rowInstalledTileMeta(row: Row): RmdfManifestTile | null {
  return parseTileMeta(row.installed_meta);
}

export function upsertRmdfManifestTile(localStore: Store, tile: RmdfManifestTile, status?: RmdfTileStatus) {
  localStore.setRow(RMDF_TILES_TABLE, tile.filename, {
    ...localStore.getRow(RMDF_TILES_TABLE, tile.filename),
    tile_id: tile.filename,
    filename: tile.filename,
    checksum: tile.checksum,
    meta: JSON.stringify(tile),
    local_path: getRmdfTilePath(tile.filename),
    status: status ?? localStore.getCell(RMDF_TILES_TABLE, tile.filename, 'status') ?? 'remoteOnly',
    size_bytes: tile.size_bytes,
  });
}

export function queueRmdfManifestTile(localStore: Store, tile: RmdfManifestTile, priority: number, queuedAt: string) {
  upsertRmdfManifestTile(localStore, tile, 'queued');
  localStore.setRow(RMDF_DOWNLOAD_QUEUE_TABLE, getRmdfDownloadQueueRowId(tile.filename), {
    tile_id: tile.filename,
    filename: tile.filename,
    priority,
    state: 'queued',
    attempts: 0,
    expected_bytes: tile.size_bytes,
    downloaded_bytes: 0,
    temp_path: getRmdfTempTilePath(tile.filename),
    queued_at: queuedAt,
    updated_at: queuedAt,
  });
}

export function markRmdfManifestTileDownloaded(localStore: Store, tile: RmdfManifestTile, downloadedAt: string) {
  localStore.setRow(RMDF_TILES_TABLE, tile.filename, {
    ...localStore.getRow(RMDF_TILES_TABLE, tile.filename),
    tile_id: tile.filename,
    filename: tile.filename,
    checksum: tile.checksum,
    meta: JSON.stringify(tile),
    installed_checksum: tile.checksum,
    installed_meta: JSON.stringify(tile),
    local_path: getRmdfTilePath(tile.filename),
    status: 'downloaded',
    size_bytes: tile.size_bytes,
    downloaded_at: downloadedAt,
    last_checked_at: downloadedAt,
  });
  localStore.delCell(RMDF_TILES_TABLE, tile.filename, 'error_message');
}

export function markRmdfTileRemoteOnly(localStore: Store, filename: string, checkedAt?: string) {
  const row = { ...localStore.getRow(RMDF_TILES_TABLE, filename) };
  delete row.installed_checksum;
  delete row.installed_meta;
  delete row.downloaded_at;
  localStore.setRow(RMDF_TILES_TABLE, filename, {
    ...row,
    tile_id: filename,
    filename,
    local_path: getRmdfTilePath(filename),
    status: 'remoteOnly',
    selected: false,
    ...(checkedAt ? { last_checked_at: checkedAt } : null),
  });
}

export function setRmdfTilesSelected(localStore: Store, tiles: RmdfManifestTile[], selected: boolean) {
  localStore.transaction(() => {
    for (const tile of tiles) {
      if (selected) upsertRmdfManifestTile(localStore, tile);
      if (selected || localStore.hasRow(RMDF_TILES_TABLE, tile.filename)) {
        localStore.setCell(RMDF_TILES_TABLE, tile.filename, 'selected', selected);
      }
    }
  });
}

export function clearRmdfTileSelection(localStore: Store, filenames: Iterable<string>) {
  for (const filename of filenames) {
    if (localStore.hasRow(RMDF_TILES_TABLE, filename)) {
      localStore.setCell(RMDF_TILES_TABLE, filename, 'selected', false);
    }
  }
}
