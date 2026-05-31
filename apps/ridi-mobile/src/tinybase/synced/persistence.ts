import { SYNCED_DATABASE_NAME, SYNCED_STORE_TABLE_NAME } from '../ids';
import { createSqlitePersistence, type SqlitePersistence } from '../persistence';
import type { SyncedStore } from './store';

export type SyncedPersistence = SqlitePersistence;

export function createSyncedPersistence(store: SyncedStore): Promise<SyncedPersistence> {
  return createSqlitePersistence({
    store,
    databaseName: SYNCED_DATABASE_NAME,
    tableName: SYNCED_STORE_TABLE_NAME,
    logPrefix: 'tinybase:synced',
  });
}
