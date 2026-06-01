export type RouteFeatureKind = 'coordinate' | 'poi' | 'name';
export type RoutePointSource = 'current_location' | 'map_selection';
export type RouteRulePresetId = 'dualsport' | 'adv' | 'touring';
export type RouteGenerationStatus = 'succeeded' | 'failed';

export type RoutePoint = {
  latitude: number;
  longitude: number;
  label: string;
  source: RoutePointSource;
  featureKind?: RouteFeatureKind;
  tileId?: string;
};

export type RouteDraft = {
  start?: RoutePoint;
  finish?: RoutePoint;
  rulePresetId: RouteRulePresetId;
};

export type RoutePlanRow = {
  plan_id: string;
  created_at: string;
  updated_at?: string;
  deleted_at?: string;
  schema_version: number;
  start_label: string;
  start_latitude: number;
  start_longitude: number;
  start_source: RoutePointSource;
  start_feature_kind?: RouteFeatureKind;
  start_tile_id: string;
  finish_label: string;
  finish_latitude: number;
  finish_longitude: number;
  finish_source: RoutePointSource;
  finish_feature_kind?: RouteFeatureKind;
  finish_tile_id: string;
  rule_preset_id: RouteRulePresetId;
  rule_label: string;
  rules_json: string;
  route_count: number;
  total_distance_m: number;
  selected_route_index: number;
  generation_status: RouteGenerationStatus;
  error_code?: string;
  error_message?: string;
  error_details?: string;
};

export type RouteRow = {
  route_id: string;
  plan_id: string;
  route_index: number;
  created_at: string;
  updated_at?: string;
  deleted_at?: string;
  schema_version: number;
  length_m: number;
  score: number;
  junction_count: number;
  cluster?: number;
  coords_json: string;
  approximated_route_json?: string;
  highway_stats_json: string;
  surface_stats_json: string;
  smoothness_stats_json: string;
  details_json: string;
};

export type NormalizedRoute = {
  routeIndex: number;
  coords: [number, number][];
  lengthM: number;
  score: number;
  junctionCount: number;
  cluster?: number;
  approximatedRoute?: unknown;
  highwayStats: unknown;
  surfaceStats: unknown;
  smoothnessStats: unknown;
  details: unknown;
};
