import type { Store } from 'tinybase';

import { createVersionedStore } from '../store';
import { localTablesSchema, localValuesSchema } from './schema';

export type LocalStore = Store;

export function createLocalStore(): LocalStore {
  return createVersionedStore({
    tablesSchema: localTablesSchema,
    valuesSchema: localValuesSchema,
    schemaVersionValueId: 'local_schema_version',
    schemaVersion: 1,
  });
}
