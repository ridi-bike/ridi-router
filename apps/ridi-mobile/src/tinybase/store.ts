import { createStore, type Store } from 'tinybase';

type CreateVersionedStoreOptions = {
  tablesSchema: Parameters<Store['setTablesSchema']>[0];
  valuesSchema: Parameters<Store['setValuesSchema']>[0];
  schemaVersionValueId: string;
  schemaVersion: number;
};

export function createVersionedStore({ tablesSchema, valuesSchema, schemaVersionValueId, schemaVersion }: CreateVersionedStoreOptions): Store {
  return createStore().setTablesSchema(tablesSchema).setValuesSchema(valuesSchema).setValue(schemaVersionValueId, schemaVersion);
}
