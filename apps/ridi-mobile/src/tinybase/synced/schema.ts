import { createZodSchematizer } from 'tinybase/schematizers/schematizer-zod';
import { z } from 'zod';

export const routePointSourceSchema = z.enum(['current_location', 'map_selection']);
export const routeFeatureKindSchema = z.enum(['coordinate', 'poi', 'name']);
export const routeRulePresetIdSchema = z.enum(['dualsport', 'adv', 'touring']);
export const routeGenerationStatusSchema = z.enum(['succeeded', 'failed']);

export const syncedEntityBaselineSchema = z.object({
  created_at: z.string(),
  updated_at: z.string().optional(),
  deleted_at: z.string().optional(),
  schema_version: z.number().default(1),
});

export const syncedTableSchemas = {
  plans: syncedEntityBaselineSchema.extend({
    plan_id: z.string(),
    start_label: z.string(),
    start_latitude: z.number(),
    start_longitude: z.number(),
    start_source: routePointSourceSchema,
    start_feature_kind: routeFeatureKindSchema.optional(),
    start_tile_id: z.string(),
    finish_label: z.string(),
    finish_latitude: z.number(),
    finish_longitude: z.number(),
    finish_source: routePointSourceSchema,
    finish_feature_kind: routeFeatureKindSchema.optional(),
    finish_tile_id: z.string(),
    rule_preset_id: routeRulePresetIdSchema,
    rule_label: z.string(),
    rules_json: z.string(),
    route_count: z.number().default(0),
    total_distance_m: z.number().default(0),
    selected_route_index: z.number().default(0),
    generation_status: routeGenerationStatusSchema,
    error_code: z.string().optional(),
    error_message: z.string().optional(),
    error_details: z.string().optional(),
  }),
  routes: syncedEntityBaselineSchema.extend({
    route_id: z.string(),
    plan_id: z.string(),
    route_index: z.number(),
    length_m: z.number().default(0),
    score: z.number().default(0),
    junction_count: z.number().default(0),
    cluster: z.number().optional(),
    coords_json: z.string(),
    approximated_route_json: z.string().optional(),
    highway_stats_json: z.string(),
    surface_stats_json: z.string(),
    smoothness_stats_json: z.string(),
    details_json: z.string(),
  }),
};

export const syncedValueSchemas = {
  synced_schema_version: z.number().default(1),
  sync_client_id: z.string().optional(),
  last_successful_sync_at: z.string().optional(),
};

const schematizer = createZodSchematizer();

export const syncedTablesSchema = schematizer.toTablesSchema(syncedTableSchemas);
export const syncedValuesSchema = schematizer.toValuesSchema(syncedValueSchemas);

export type RoutePlanSource = z.infer<typeof routePointSourceSchema>;
export type RoutePlanFeatureKind = z.infer<typeof routeFeatureKindSchema>;
export type SyncedRouteRulePresetId = z.infer<typeof routeRulePresetIdSchema>;
export type SyncedRouteGenerationStatus = z.infer<typeof routeGenerationStatusSchema>;
