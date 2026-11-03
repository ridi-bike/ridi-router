import { getRouteRulePreset } from './rulePresets';
import type { NormalizedRoute, RouteDraft } from './types';

export function buildRouteRequest(draft: Required<RouteDraft>) {
  return {
    mode: {
      StartFinish: {
        start: { lat: draft.start.latitude, lon: draft.start.longitude },
        finish: { lat: draft.finish.latitude, lon: draft.finish.longitude },
      },
    },
    rules: getRouteRulePreset(draft.rulePresetId).rules,
  };
}

function numberOrZero(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

function parseCoords(value: unknown): [number, number][] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((pair) => {
    if (!Array.isArray(pair) || pair.length < 2) return [];
    const [lat, lon] = pair;
    return typeof lat === 'number' && typeof lon === 'number' ? ([[lat, lon]] as [number, number][]) : [];
  });
}

function routeArray(value: unknown): unknown[] {
  if (Array.isArray(value)) return value;
  if (value && typeof value === 'object') {
    const object = value as { routes?: unknown; value?: unknown; result?: unknown };
    if (Array.isArray(object.routes)) return object.routes;
    if (Array.isArray(object.value)) return object.value;
    if (Array.isArray(object.result)) return object.result;
  }
  return [];
}

export function normalizeRouteResult(value: unknown): NormalizedRoute[] {
  return routeArray(value).map((route, routeIndex) => {
    const object = route && typeof route === 'object' ? (route as Record<string, unknown>) : {};
    const stats = object.stats && typeof object.stats === 'object' ? (object.stats as Record<string, unknown>) : {};
    const cluster = typeof stats.cluster === 'number' && Number.isFinite(stats.cluster) ? stats.cluster : undefined;
    return {
      routeIndex,
      coords: parseCoords(object.coords),
      lengthM: numberOrZero(stats.len_m),
      score: numberOrZero(stats.score),
      junctionCount: numberOrZero(stats.junction_count),
      cluster,
      approximatedRoute: stats.approximated_route,
      highwayStats: stats.highway ?? {},
      surfaceStats: stats.surface ?? {},
      smoothnessStats: stats.smoothness ?? {},
      details: object,
    };
  });
}

export function stringifyRouteRequest(draft: Required<RouteDraft>): string {
  return JSON.stringify(buildRouteRequest(draft));
}
