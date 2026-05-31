import { LOCAL_DATABASE_NAME, LOCAL_STORE_TABLE_NAME } from '../ids';
import { createSqlitePersistence, type SqlitePersistence } from '../persistence';
import type { LocalStore } from './store';

export type LocalPersistence = SqlitePersistence;

export function createLocalPersistence(store: LocalStore): Promise<LocalPersistence> {
  return createSqlitePersistence({
    store,
    databaseName: LOCAL_DATABASE_NAME,
    tableName: LOCAL_STORE_TABLE_NAME,
    logPrefix: 'tinybase:local',
  });
}
