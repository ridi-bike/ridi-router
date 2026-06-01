import { coordsToGeoJson } from './format';
import type { NormalizedRoute, RoutePoint } from './types';

export type FeatureCollection = {
  type: 'FeatureCollection';
  features: Record<string, unknown>[];
};

export function planningGuideGeoJson(start?: RoutePoint, finish?: RoutePoint): FeatureCollection {
  if (!start || !finish) return { type: 'FeatureCollection', features: [] };
  return {
    type: 'FeatureCollection',
    features: [{
      type: 'Feature',
      properties: { kind: 'planning_direction' },
      geometry: { type: 'LineString', coordinates: [[start.longitude, start.latitude], [finish.longitude, finish.latitude]] },
    }],
  };
}

export function generatedRoutesGeoJson(routes: NormalizedRoute[], selectedRouteIndex: number, selected: boolean): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: routes.map((route) => ({
      type: 'Feature',
      properties: { index: route.routeIndex, selected: selected && route.routeIndex === selectedRouteIndex },
      geometry: { type: 'LineString', coordinates: coordsToGeoJson(route.coords) },
    })),
  };
}
