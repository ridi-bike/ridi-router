import { createZodSchematizer } from 'tinybase/schematizers/schematizer-zod';
import { z } from 'zod';

export const rmdfTileStatusSchema = z.enum(['remoteOnly', 'queued', 'downloading', 'downloaded', 'failed']);
export const rmdfDownloadQueueStateSchema = z.enum(['queued', 'running', 'paused', 'failed']);

export const localTableSchemas = {
  rmdf_tiles: z.object({
    tile_id: z.string(),
    filename: z.string(),
    // Release metadata the tile was last selected or queued with (JSON RmdfManifestTile).
    checksum: z.string().optional(),
    meta: z.string().optional(),
    // What is on disk: checksum is the installed tile version, meta feeds the router manifest.
    installed_checksum: z.string().optional(),
    installed_meta: z.string().optional(),
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
    downloaded_bytes: z.number().default(0),
    temp_path: z.string().optional(),
    queued_at: z.string(),
    started_at: z.string().optional(),
    updated_at: z.string().optional(),
    error_message: z.string().optional(),
  }),
};

export const localValueSchemas = {
  local_schema_version: z.number().default(1),
  rmdf_last_reconciled_at: z.string().optional(),
  // Tile grid of the release the installed tiles come from; the router manifest needs it.
  rmdf_tile_size_degrees: z.number().optional(),
  // RMDF format the installed tiles were last checked against (see reconcile).
  rmdf_checked_format_version: z.number().optional(),
  rmdf_download_warning: z.string().optional(),
  rmdf_download_last_summary: z.string().optional(),
};

const schematizer = createZodSchematizer();

export const localTablesSchema = schematizer.toTablesSchema(localTableSchemas);
export const localValuesSchema = schematizer.toValuesSchema(localValueSchemas);

export type RmdfTileStatus = z.infer<typeof rmdfTileStatusSchema>;
export type RmdfDownloadQueueState = z.infer<typeof rmdfDownloadQueueStateSchema>;
