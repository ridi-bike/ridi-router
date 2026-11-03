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

/** The tile is not on disk; a row without a download, failure or selection is just dropped. */
export function markRmdfTileRemoteOnly(localStore: Store, filename: string) {
  localStore.delRow(RMDF_TILES_TABLE, filename);
}

/** A row that only records a selection carries no state worth keeping once deselected. */
function isSelectionOnlyRow(localStore: Store, filename: string): boolean {
  const row = localStore.getRow(RMDF_TILES_TABLE, filename);
  return !row.installed_meta && row.status !== 'queued' && row.status !== 'downloading' && row.status !== 'failed';
}

export function setRmdfTilesSelected(localStore: Store, tiles: RmdfManifestTile[], selected: boolean) {
  localStore.transaction(() => {
    for (const tile of tiles) {
      if (selected) {
        upsertRmdfManifestTile(localStore, tile);
        localStore.setCell(RMDF_TILES_TABLE, tile.filename, 'selected', true);
      } else {
        clearRmdfTileSelection(localStore, [tile.filename]);
      }
    }
  });
}

export function clearRmdfTileSelection(localStore: Store, filenames: Iterable<string>) {
  localStore.transaction(() => {
    for (const filename of filenames) {
      if (!localStore.hasRow(RMDF_TILES_TABLE, filename)) continue;
      if (isSelectionOnlyRow(localStore, filename)) localStore.delRow(RMDF_TILES_TABLE, filename);
      else localStore.setCell(RMDF_TILES_TABLE, filename, 'selected', false);
    }
  });
}
