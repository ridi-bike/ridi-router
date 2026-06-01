import type { Store } from 'tinybase';

import { getRouteRulePreset } from './rulePresets';
import type { NormalizedRoute, RouteDraft, RoutePlanRow, RouteRow } from './types';

export const ROUTE_PLANS_TABLE = 'plans';
export const ROUTES_TABLE = 'routes';
const SCHEMA_VERSION = 1;

function randomHex(bytes: number): string {
  const buffer = new Uint8Array(bytes);
  const cryptoLike = globalThis.crypto as { getRandomValues?: (array: Uint8Array) => Uint8Array } | undefined;
  if (cryptoLike?.getRandomValues) {
    cryptoLike.getRandomValues(buffer);
  } else {
    for (let index = 0; index < buffer.length; index += 1) {
      buffer[index] = Math.floor(Math.random() * 256);
    }
  }
  return Array.from(buffer, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function createRoutePlanId(createdAt: string): string {
  return `plan:${createdAt.replace(/[^0-9]/g, '')}:${randomHex(10)}`;
}

function cleanRow(row: Record<string, unknown>): Record<string, string | number | boolean> {
  return Object.fromEntries(Object.entries(row).filter(([, value]) => value !== undefined)) as Record<string, string | number | boolean>;
}

type PersistArgs = {
  store: Store;
  draft: Required<RouteDraft>;
  routes: NormalizedRoute[];
  status: 'succeeded' | 'failed';
  error?: { code?: string; message?: string; details?: string };
};

export function routeRowId(planId: string, routeIndex: number): string {
  return `${planId}:${routeIndex}`;
}

export function typedPlanRow(row: Record<string, unknown>): RoutePlanRow {
  return row as RoutePlanRow;
}

export function typedRouteRow(row: Record<string, unknown>): RouteRow {
  return row as RouteRow;
}

export function getPlan(store: Store, planId: string): RoutePlanRow | null {
  const row = store.getRow(ROUTE_PLANS_TABLE, planId);
  return Object.keys(row).length ? typedPlanRow(row) : null;
}

export function getRoutesForPlan(store: Store, planId: string): RouteRow[] {
  return Object.values(store.getTable(ROUTES_TABLE))
    .map((row) => typedRouteRow(row))
    .filter((row) => row.plan_id === planId && !row.deleted_at)
    .sort((a, b) => a.route_index - b.route_index);
}

export function routeRowToNormalized(row: RouteRow): NormalizedRoute {
  return {
    routeIndex: row.route_index,
    coords: JSON.parse(row.coords_json) as [number, number][],
    lengthM: row.length_m,
    score: row.score,
    junctionCount: row.junction_count,
    cluster: row.cluster,
    approximatedRoute: row.approximated_route_json ? JSON.parse(row.approximated_route_json) : undefined,
    highwayStats: JSON.parse(row.highway_stats_json || '{}'),
    surfaceStats: JSON.parse(row.surface_stats_json || '{}'),
    smoothnessStats: JSON.parse(row.smoothness_stats_json || '{}'),
    details: JSON.parse(row.details_json || '{}'),
  };
}

export function persistRouteGeneration(args: PersistArgs): RoutePlanRow {
  const now = new Date().toISOString();
  const planId = createRoutePlanId(now);
  const preset = getRouteRulePreset(args.draft.rulePresetId);
  const totalDistance = args.status === 'succeeded' ? args.routes.reduce((sum, route) => sum + route.lengthM, 0) : 0;
  const plan: RoutePlanRow = {
    plan_id: planId,
    created_at: now,
    updated_at: now,
    schema_version: SCHEMA_VERSION,
    start_label: args.draft.start.label,
    start_latitude: args.draft.start.latitude,
    start_longitude: args.draft.start.longitude,
    start_source: args.draft.start.source,
    start_feature_kind: args.draft.start.featureKind,
    start_tile_id: args.draft.start.tileId ?? '',
    finish_label: args.draft.finish.label,
    finish_latitude: args.draft.finish.latitude,
    finish_longitude: args.draft.finish.longitude,
    finish_source: args.draft.finish.source,
    finish_feature_kind: args.draft.finish.featureKind,
    finish_tile_id: args.draft.finish.tileId ?? '',
    rule_preset_id: preset.id,
    rule_label: preset.label,
    rules_json: JSON.stringify(preset.rules),
    route_count: args.status === 'succeeded' ? args.routes.length : 0,
    total_distance_m: totalDistance,
    selected_route_index: 0,
    generation_status: args.status,
    error_code: args.error?.code,
    error_message: args.error?.message,
    error_details: args.error?.details,
  };

  args.store.setRow(ROUTE_PLANS_TABLE, planId, cleanRow(plan as unknown as Record<string, unknown>));

  if (args.status === 'succeeded') {
    for (const route of args.routes) {
      const routeId = routeRowId(planId, route.routeIndex);
      const row: RouteRow = {
        route_id: routeId,
        plan_id: planId,
        route_index: route.routeIndex,
        created_at: now,
        updated_at: now,
        schema_version: SCHEMA_VERSION,
        length_m: route.lengthM,
        score: route.score,
        junction_count: route.junctionCount,
        cluster: route.cluster,
        coords_json: JSON.stringify(route.coords),
        approximated_route_json: route.approximatedRoute === undefined ? undefined : JSON.stringify(route.approximatedRoute),
        highway_stats_json: JSON.stringify(route.highwayStats ?? {}),
        surface_stats_json: JSON.stringify(route.surfaceStats ?? {}),
        smoothness_stats_json: JSON.stringify(route.smoothnessStats ?? {}),
        details_json: JSON.stringify(route.details ?? {}),
      };
      args.store.setRow(ROUTES_TABLE, routeId, cleanRow(row as unknown as Record<string, unknown>));
    }
  }

  return plan;
}

export function updateSelectedRouteIndex(store: Store, planId: string, selectedRouteIndex: number) {
  store.setPartialRow(ROUTE_PLANS_TABLE, planId, {
    selected_route_index: selectedRouteIndex,
    updated_at: new Date().toISOString(),
  });
}

export function softDeletePlan(store: Store, planId: string) {
  const now = new Date().toISOString();
  store.setPartialRow(ROUTE_PLANS_TABLE, planId, { deleted_at: now, updated_at: now });
}
