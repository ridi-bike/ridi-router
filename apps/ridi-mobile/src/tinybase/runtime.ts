import * as FileSystem from 'expo-file-system/legacy';
import * as SQLite from 'expo-sqlite';
import { AppState } from 'react-native';
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

const FALLBACK_SQLITE_DIRECTORY_NAME = 'tinybase-sqlite/';
const DEFAULT_DIRECTORY_ATTEMPTS = 4;

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function getFallbackSqliteDirectory(): Promise<string | null> {
  if (!FileSystem.documentDirectory) {
    return null;
  }

  const directory = `${FileSystem.documentDirectory}${FALLBACK_SQLITE_DIRECTORY_NAME}`;
  const info = await FileSystem.getInfoAsync(directory);
  if (info.exists && !info.isDirectory) {
    await FileSystem.deleteAsync(directory, { idempotent: true });
  }
  if (!info.exists || !info.isDirectory) {
    await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
  }

  return directory;
}

/**
 * Opens a database in Expo SQLite's default directory. The "blocked" error is transient (it shows
 * up while the directory is being created), so the default location is retried before falling
 * back. Switching location silently would open a different, empty database and the persister
 * would then overwrite the user's data with an empty store; the fallback is used only when the
 * default location keeps failing.
 */
async function openTinyBaseDatabase(databaseName: string, logPrefix: string): Promise<SQLite.SQLiteDatabase> {
  let lastError: unknown;
  for (let attempt = 0; attempt < DEFAULT_DIRECTORY_ATTEMPTS; attempt += 1) {
    try {
      return await SQLite.openDatabaseAsync(databaseName);
    } catch (error) {
      if (!isBlockedSqliteDirectoryError(error)) throw error;
      lastError = error;
      await sleep(100 * (attempt + 1));
    }
  }

  const fallbackDirectory = await getFallbackSqliteDirectory();
  if (!fallbackDirectory) throw lastError;
  console.warn(`[${logPrefix}] Expo SQLite default directory stays blocked; opening ${databaseName} in ${fallbackDirectory}.`, lastError);
  return SQLite.openDatabaseAsync(databaseName, undefined, fallbackDirectory);
}

const SAVE_DELAY_MS = 500;
const SAVE_MAX_DELAY_MS = 2000;

/**
 * Like the persister's auto-save, but coalesced. The store is saved as one JSON document, and
 * auto-save writes all of it after every transaction; a large download (two transactions per tile)
 * would rewrite megabytes thousands of times. Saves wait until changes settle (at most
 * SAVE_MAX_DELAY_MS after the first one) and are flushed when the app leaves the foreground.
 */
async function startCoalescedAutoSave(store: Store, persister: ExpoSqlitePersister): Promise<void> {
  await persister.save();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let firstChangeAt = 0;
  const flush = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    firstChangeAt = 0;
    void persister.save();
  };
  store.addDidFinishTransactionListener(() => {
    const now = Date.now();
    if (!firstChangeAt) firstChangeAt = now;
    if (timer) clearTimeout(timer);
    timer = setTimeout(flush, Math.max(0, Math.min(SAVE_DELAY_MS, firstChangeAt + SAVE_MAX_DELAY_MS - now)));
  });
  AppState.addEventListener('change', (state) => {
    if (state !== 'active' && timer) flush();
  });
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

  // One at a time: opening both at once races on creating the SQLite directory.
  const localPersistence = await createSqlitePersistence(localStore, LOCAL_DATABASE_NAME, LOCAL_STORE_TABLE_NAME, 'tinybase:local');
  const syncedPersistence = await createSqlitePersistence(syncedStore, SYNCED_DATABASE_NAME, SYNCED_STORE_TABLE_NAME, 'tinybase:synced');

  await Promise.all([localPersistence.persister.load(), syncedPersistence.persister.load()]);

  localStore.setValue('local_schema_version', 1);
  syncedStore.setValue('synced_schema_version', 1);

  await reconcileLocalRmdfTiles(localStore);
  const rmdfDownloadManager = new RmdfDownloadManager(localStore);
  await rmdfDownloadManager.hydrate();

  await Promise.all([startCoalescedAutoSave(localStore, localPersistence.persister), startCoalescedAutoSave(syncedStore, syncedPersistence.persister)]);

  if (options.startQueuedDownloads ?? true) void rmdfDownloadManager.processQueuedDownloads();

  return { localStore, syncedStore, localPersistence, syncedPersistence, rmdfDownloadManager };
}

const RUNTIME_KEY = '__ridiTinyBaseRuntime';

/**
 * The app's single runtime. It lives on `globalThis` so a provider remount (Fast Refresh re-running
 * modules) reuses it: a second runtime would share the Expo SQLite connection with the first,
 * their transactions interleave, the new persister finds no table, starts empty and autosaves that
 * over the user's data.
 */
export function getTinyBaseRuntime(): Promise<TinyBaseRuntime> {
  const holder = globalThis as typeof globalThis & { [RUNTIME_KEY]?: Promise<TinyBaseRuntime> };
  let runtime = holder[RUNTIME_KEY];
  if (!runtime) {
    runtime = initializeTinyBaseRuntime();
    runtime.catch(() => {
      if (holder[RUNTIME_KEY] === runtime) delete holder[RUNTIME_KEY];
    });
    holder[RUNTIME_KEY] = runtime;
  }
  return runtime;
}

export async function destroyTinyBaseRuntime(runtime: TinyBaseRuntime): Promise<void> {
  await Promise.all([runtime.localPersistence.persister.save(), runtime.syncedPersistence.persister.save()]);
  await Promise.all([runtime.localPersistence.persister.destroy(), runtime.syncedPersistence.persister.destroy()]);
}
