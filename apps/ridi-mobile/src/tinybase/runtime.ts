import { migrateLegacyRmdfQueueToLocalStore, reconcileLocalRmdfTiles } from './local/rmdfReconcile';
import { createLocalPersistence, type LocalPersistence } from './local/persistence';
import { createLocalStore, type LocalStore } from './local/store';
import { createSyncedPersistence, type SyncedPersistence } from './synced/persistence';
import { createSyncedStore, type SyncedStore } from './synced/store';
import { rmdfDownloadManager } from '@/rmdf/downloads';

export type TinyBaseRuntime = {
  localStore: LocalStore;
  syncedStore: SyncedStore;
  localPersistence: LocalPersistence;
  syncedPersistence: SyncedPersistence;
};

type InitializeTinyBaseRuntimeOptions = {
  startQueuedDownloads?: boolean;
};

export async function initializeTinyBaseRuntime(options: InitializeTinyBaseRuntimeOptions = {}): Promise<TinyBaseRuntime> {
  const localStore = createLocalStore();
  const syncedStore = createSyncedStore();

  const [localPersistence, syncedPersistence] = await Promise.all([createLocalPersistence(localStore), createSyncedPersistence(syncedStore)]);

  await Promise.all([localPersistence.persister.load(), syncedPersistence.persister.load()]);

  localStore.setValue('local_schema_version', 1);
  syncedStore.setValue('synced_schema_version', 1);

  await migrateLegacyRmdfQueueToLocalStore(localStore);
  await reconcileLocalRmdfTiles(localStore);
  rmdfDownloadManager.attachLocalStore(localStore);
  await rmdfDownloadManager.hydrate();

  await Promise.all([localPersistence.persister.startAutoSave(), syncedPersistence.persister.startAutoSave()]);

  if (options.startQueuedDownloads ?? true) void rmdfDownloadManager.processQueuedDownloads();

  return { localStore, syncedStore, localPersistence, syncedPersistence };
}

export async function destroyTinyBaseRuntime(runtime: TinyBaseRuntime): Promise<void> {
  await Promise.all([runtime.localPersistence.persister.save(), runtime.syncedPersistence.persister.save()]);
  rmdfDownloadManager.detachLocalStore();
  await Promise.all([runtime.localPersistence.persister.destroy(), runtime.syncedPersistence.persister.destroy()]);
}
