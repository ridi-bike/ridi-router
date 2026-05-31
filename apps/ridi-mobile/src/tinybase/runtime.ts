import * as FileSystem from 'expo-file-system/legacy';
import * as SQLite from 'expo-sqlite';
import { createStore, type Store } from 'tinybase';
import { createExpoSqlitePersister, type ExpoSqlitePersister } from 'tinybase/persisters/persister-expo-sqlite';

import { LOCAL_DATABASE_NAME, LOCAL_STORE_TABLE_NAME, SYNCED_DATABASE_NAME, SYNCED_STORE_TABLE_NAME } from './ids';
import { localTablesSchema, localValuesSchema } from './local/schema';
import { reconcileLocalRmdfTiles } from './local/rmdfReconcile';
import { syncedTablesSchema, syncedValuesSchema } from './synced/schema';
import { RmdfDownloadManager } from '@/rmdf/downloads';

export type TinyBaseRuntime = {
  localStore: Store;
  syncedStore: Store;
  localPersistence: SqlitePersistence;
  syncedPersistence: SqlitePersistence;
  rmdfDownloadManager: RmdfDownloadManager;
};

type SqlitePersistence = {
  db: SQLite.SQLiteDatabase;
  persister: ExpoSqlitePersister;
};

type InitializeTinyBaseRuntimeOptions = {
  startQueuedDownloads?: boolean;
};

function createVersionedStore(tablesSchema: Parameters<Store['setTablesSchema']>[0], valuesSchema: Parameters<Store['setValuesSchema']>[0], schemaVersionValueId: string): Store {
  return createStore().setTablesSchema(tablesSchema).setValuesSchema(valuesSchema).setValue(schemaVersionValueId, 1);
}

function isBlockedSqliteDirectoryError(error: unknown): boolean {
  return error instanceof Error && error.message.includes('Path already points to a non-normal file');
}

async function getFallbackSqliteDirectory(): Promise<string | null> {
  if (!FileSystem.documentDirectory) {
    return null;
  }

  const directory = `${FileSystem.documentDirectory}tinybase-sqlite/`;
  const info = await FileSystem.getInfoAsync(directory);
  if (info.exists && !info.isDirectory) {
    await FileSystem.deleteAsync(directory, { idempotent: true });
  }
  if (!info.exists || !info.isDirectory) {
    await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
  }

  return directory;
}

async function openTinyBaseDatabase(databaseName: string, logPrefix: string): Promise<SQLite.SQLiteDatabase> {
  try {
    return await SQLite.openDatabaseAsync(databaseName);
  } catch (error) {
    if (!isBlockedSqliteDirectoryError(error)) {
      throw error;
    }

    const fallbackDirectory = await getFallbackSqliteDirectory();
    if (!fallbackDirectory) {
      throw error;
    }

    if (__DEV__) {
      console.warn(`[${logPrefix}] Expo SQLite default directory is blocked by a file. Retrying in ${fallbackDirectory}.`, error);
    }
    return SQLite.openDatabaseAsync(databaseName, undefined, fallbackDirectory);
  }
}

async function createSqlitePersistence(store: Store, databaseName: string, tableName: string, logPrefix: string): Promise<SqlitePersistence> {
  const db = await openTinyBaseDatabase(databaseName, logPrefix);
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

export async function initializeTinyBaseRuntime(options: InitializeTinyBaseRuntimeOptions = {}): Promise<TinyBaseRuntime> {
  const localStore = createVersionedStore(localTablesSchema, localValuesSchema, 'local_schema_version');
  const syncedStore = createVersionedStore(syncedTablesSchema, syncedValuesSchema, 'synced_schema_version');

  const [localPersistence, syncedPersistence] = await Promise.all([
    createSqlitePersistence(localStore, LOCAL_DATABASE_NAME, LOCAL_STORE_TABLE_NAME, 'tinybase:local'),
    createSqlitePersistence(syncedStore, SYNCED_DATABASE_NAME, SYNCED_STORE_TABLE_NAME, 'tinybase:synced'),
  ]);

  await Promise.all([localPersistence.persister.load(), syncedPersistence.persister.load()]);

  localStore.setValue('local_schema_version', 1);
  syncedStore.setValue('synced_schema_version', 1);

  await reconcileLocalRmdfTiles(localStore);
  const rmdfDownloadManager = new RmdfDownloadManager(localStore);
  await rmdfDownloadManager.hydrate();

  await Promise.all([localPersistence.persister.startAutoSave(), syncedPersistence.persister.startAutoSave()]);

  if (options.startQueuedDownloads ?? true) void rmdfDownloadManager.processQueuedDownloads();

  return { localStore, syncedStore, localPersistence, syncedPersistence, rmdfDownloadManager };
}

export async function destroyTinyBaseRuntime(runtime: TinyBaseRuntime): Promise<void> {
  await Promise.all([runtime.localPersistence.persister.save(), runtime.syncedPersistence.persister.save()]);
  await Promise.all([runtime.localPersistence.persister.destroy(), runtime.syncedPersistence.persister.destroy()]);
}
