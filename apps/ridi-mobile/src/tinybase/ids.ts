export const LOCAL_STORE_ID = 'localStore';
export const SYNCED_STORE_ID = 'syncedStore';

export const LOCAL_DATABASE_NAME = 'ridi-local-tinybase.db';
export const SYNCED_DATABASE_NAME = 'ridi-synced-tinybase.db';

export const LOCAL_STORE_TABLE_NAME = 'local_store';
export const SYNCED_STORE_TABLE_NAME = 'synced_store';

export const RMDF_TILES_TABLE = 'rmdf_tiles';
export const RMDF_DOWNLOAD_QUEUE_TABLE = 'rmdf_download_queue';

export function getRmdfDownloadQueueRowId(tileId: string): string {
  return `tile:${tileId}`;
}
