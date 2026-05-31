import { createZodSchematizer } from 'tinybase/schematizers/schematizer-zod';
import { z } from 'zod';

export const rmdfTileStatusSchema = z.enum(['remoteOnly', 'queued', 'downloading', 'downloaded', 'failed']);
export const rmdfDownloadQueueStateSchema = z.enum(['queued', 'running', 'paused', 'failed']);

export const localTableSchemas = {
  rmdf_tiles: z.object({
    tile_id: z.string(),
    filename: z.string(),
    checksum: z.string().optional(),
    local_path: z.string().optional(),
    status: rmdfTileStatusSchema.default('remoteOnly'),
    selected: z.boolean().default(false),
    size_bytes: z.number().optional(),
    downloaded_at: z.string().optional(),
    last_checked_at: z.string().optional(),
    error_message: z.string().optional(),
  }),
  rmdf_download_queue: z.object({
    tile_id: z.string(),
    filename: z.string(),
    priority: z.number().default(0),
    state: rmdfDownloadQueueStateSchema.default('queued'),
    attempts: z.number().default(0),
    expected_bytes: z.number().optional(),
    queued_at: z.string(),
    started_at: z.string().optional(),
    updated_at: z.string().optional(),
    error_message: z.string().optional(),
  }),
};

export const localValueSchemas = {
  local_schema_version: z.number().default(1),
  rmdf_last_reconciled_at: z.string().optional(),
  rmdf_downloads_active: z.boolean().default(false),
  rmdf_download_total_count: z.number().default(0),
  rmdf_download_completed_count: z.number().default(0),
  rmdf_download_total_bytes: z.number().default(0),
  rmdf_download_completed_bytes: z.number().default(0),
  rmdf_download_warning: z.string().optional(),
  rmdf_download_last_summary: z.string().optional(),
};

const schematizer = createZodSchematizer();

export const localTablesSchema = schematizer.toTablesSchema(localTableSchemas);
export const localValuesSchema = schematizer.toValuesSchema(localValueSchemas);

export type RmdfTileStatus = z.infer<typeof rmdfTileStatusSchema>;
export type RmdfDownloadQueueState = z.infer<typeof rmdfDownloadQueueStateSchema>;
