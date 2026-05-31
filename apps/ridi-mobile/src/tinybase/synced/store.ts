import type { Store } from 'tinybase';

import { createVersionedStore } from '../store';
import { syncedTablesSchema, syncedValuesSchema } from './schema';

export type SyncedStore = Store;

export function createSyncedStore(): SyncedStore {
  return createVersionedStore({
    tablesSchema: syncedTablesSchema,
    valuesSchema: syncedValuesSchema,
    schemaVersionValueId: 'synced_schema_version',
    schemaVersion: 1,
  });
}
