import type { NormalizedRoute, RoutePlanRow, RoutePoint } from './types';

export function formatCoordinate(latitude: number, longitude: number): string {
  return `${latitude.toFixed(6)}, ${longitude.toFixed(6)}`;
}

export function formatDistance(meters: number): string {
  if (!Number.isFinite(meters) || meters <= 0) return '0 m';
  if (meters >= 1000) return `${(meters / 1000).toFixed(1)} km`;
  return `${Math.round(meters)} m`;
}

export function labelForRoutePoint(point: Pick<RoutePoint, 'label' | 'latitude' | 'longitude'>): string {
  return point.label.trim().length > 0 ? point.label : formatCoordinate(point.latitude, point.longitude);
}

export function planStartPoint(plan: RoutePlanRow): RoutePoint {
  return {
    latitude: plan.start_latitude,
    longitude: plan.start_longitude,
    label: plan.start_label,
    source: plan.start_source,
    featureKind: plan.start_feature_kind,
    tileId: plan.start_tile_id,
  };
}

export function planFinishPoint(plan: RoutePlanRow): RoutePoint {
  return {
    latitude: plan.finish_latitude,
    longitude: plan.finish_longitude,
    label: plan.finish_label,
    source: plan.finish_source,
    featureKind: plan.finish_feature_kind,
    tileId: plan.finish_tile_id,
  };
}

export function coordsToGeoJson(coords: [number, number][]): [number, number][] {
  return coords.map(([lat, lon]) => [lon, lat]);
}

export function routeBounds(routes: NormalizedRoute[]): [number, number, number, number] | null {
  let west = Infinity;
  let south = Infinity;
  let east = -Infinity;
  let north = -Infinity;
  for (const route of routes) {
    for (const [lat, lon] of route.coords) {
      west = Math.min(west, lon);
      south = Math.min(south, lat);
      east = Math.max(east, lon);
      north = Math.max(north, lat);
    }
  }
  return Number.isFinite(west) ? [west, south, east, north] : null;
}

export function endpointBounds(start: RoutePoint, finish: RoutePoint): [number, number, number, number] {
  return [
    Math.min(start.longitude, finish.longitude),
    Math.min(start.latitude, finish.latitude),
    Math.max(start.longitude, finish.longitude),
    Math.max(start.latitude, finish.latitude),
  ];
}
