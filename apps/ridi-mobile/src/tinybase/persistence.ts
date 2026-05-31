import * as SQLite from 'expo-sqlite';
import { createExpoSqlitePersister, type ExpoSqlitePersister } from 'tinybase/persisters/persister-expo-sqlite';
import type { Store } from 'tinybase';

export type SqlitePersistence = {
  db: SQLite.SQLiteDatabase;
  persister: ExpoSqlitePersister;
};

type CreateSqlitePersistenceOptions = {
  store: Store;
  databaseName: string;
  tableName: string;
  logPrefix: string;
};

export async function createSqlitePersistence({ store, databaseName, tableName, logPrefix }: CreateSqlitePersistenceOptions): Promise<SqlitePersistence> {
  const db = await SQLite.openDatabaseAsync(databaseName);
  await db.execAsync('PRAGMA journal_mode = WAL;');
  const persister = createExpoSqlitePersister(
    store,
    db,
    tableName,
    __DEV__ ? (sql, params) => console.debug(`[${logPrefix}:sql]`, sql, params ?? []) : undefined,
    __DEV__ ? (error) => console.warn(`[${logPrefix}:ignored]`, error) : undefined
  );
  return { db, persister };
}
