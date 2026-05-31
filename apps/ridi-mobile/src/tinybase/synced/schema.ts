import { createZodSchematizer } from 'tinybase/schematizers/schematizer-zod';
import { z } from 'zod';

export const syncedTableSchemas = {};

export const syncedValueSchemas = {
  synced_schema_version: z.number().default(1),
  sync_client_id: z.string().optional(),
  last_successful_sync_at: z.string().optional(),
};

export const syncedEntityBaselineSchema = z.object({
  created_at: z.string(),
  updated_at: z.string().optional(),
  deleted_at: z.string().optional(),
  schema_version: z.number().default(1),
});

const schematizer = createZodSchematizer();

export const syncedTablesSchema = schematizer.toTablesSchema(syncedTableSchemas);
export const syncedValuesSchema = schematizer.toValuesSchema(syncedValueSchemas);
