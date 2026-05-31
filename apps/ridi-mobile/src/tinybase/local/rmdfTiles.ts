import type { Store } from 'tinybase';

import type { RmdfTileStatus } from './schema';
import { getRmdfDownloadQueueRowId, RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '../ids';
import { getRmdfTempTilePath, getRmdfTilePath } from '@/rmdf/storage';
import type { RmdfManifestTile } from '@/rmdf/types';

type RmdfDownloadTile = {
  filename: string;
  size_bytes: number;
  checksum?: string;
};

export function upsertRmdfManifestTile(localStore: Store, tile: RmdfManifestTile, status?: RmdfTileStatus) {
  localStore.setRow(RMDF_TILES_TABLE, tile.filename, {
    ...localStore.getRow(RMDF_TILES_TABLE, tile.filename),
    tile_id: tile.filename,
    filename: tile.filename,
    checksum: tile.checksum,
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

export function markRmdfDownloadTileDownloaded(localStore: Store, tile: RmdfDownloadTile, downloadedAt: string) {
  localStore.setRow(RMDF_TILES_TABLE, tile.filename, {
    ...localStore.getRow(RMDF_TILES_TABLE, tile.filename),
    tile_id: tile.filename,
    filename: tile.filename,
    ...(tile.checksum ? { checksum: tile.checksum } : null),
    local_path: getRmdfTilePath(tile.filename),
    status: 'downloaded',
    size_bytes: tile.size_bytes,
    downloaded_at: downloadedAt,
    last_checked_at: downloadedAt,
  });
}

export function markRmdfManifestTileDownloaded(localStore: Store, tile: RmdfManifestTile, downloadedAt: string) {
  markRmdfDownloadTileDownloaded(localStore, tile, downloadedAt);
}

export function markRmdfTileRemoteOnly(localStore: Store, filename: string, checkedAt?: string) {
  localStore.setRow(RMDF_TILES_TABLE, filename, {
    ...localStore.getRow(RMDF_TILES_TABLE, filename),
    tile_id: filename,
    filename,
    local_path: getRmdfTilePath(filename),
    status: 'remoteOnly',
    selected: false,
    ...(checkedAt ? { last_checked_at: checkedAt } : null),
  });
}

export function setRmdfTileSelected(localStore: Store, tile: RmdfManifestTile, selected: boolean) {
  upsertRmdfManifestTile(localStore, tile);
  localStore.setCell(RMDF_TILES_TABLE, tile.filename, 'selected', selected);
}

export function clearRmdfTileSelection(localStore: Store, filenames: Iterable<string>) {
  for (const filename of filenames) {
    if (localStore.hasRow(RMDF_TILES_TABLE, filename)) {
      localStore.setCell(RMDF_TILES_TABLE, filename, 'selected', false);
    }
  }
}
