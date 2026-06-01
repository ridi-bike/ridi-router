import type { RouteRulePresetId } from './types';

const priority = { action: 'priority', value: 255 } as const;
const avoid = { action: 'avoid' } as const;

const unpavedSurfaceTags = [
  'unpaved',
  'compacted',
  'fine_gravel',
  'gravel',
  'shells',
  'rock',
  'pebblestone',
  'ground',
  'dirt',
  'earth',
  'grass',
  'mud',
  'sand',
] as const;

const blockedTouringSurfaceTags = [
  ...unpavedSurfaceTags,
  'woodchips',
  'snow',
  'ice',
  'salt',
  'metal',
  'metal_grid',
  'wood',
  'stepping_stones',
  'rubber',
  'tiles',
] as const;

const allSurfaceTags = [
  'paved',
  'asphalt',
  'chipseal',
  'concrete',
  'concrete:lanes',
  'concrete:plates',
  'paving_stones',
  'paving_stones:lanes',
  'grass_paver',
  'sett',
  'unhewn_cobblestone',
  'cobblestone',
  'bricks',
  ...blockedTouringSurfaceTags,
] as const;

const allSmoothnessTags = [
  'excellent',
  'good',
  'intermediate',
  'bad',
  'very_bad',
  'horrible',
  'very_horrible',
  'impassable',
] as const;

function priorityRules<const T extends readonly string[]>(tags: T) {
  return Object.fromEntries(tags.map((tag) => [tag, priority]));
}

function avoidRules<const T extends readonly string[]>(tags: T) {
  return Object.fromEntries(tags.map((tag) => [tag, avoid]));
}

const dualsportRules = {
  highway: {
    tertiary: priority,
    unclassified: priority,
    track: priority,
    path: priority,
  },
  surface: priorityRules(unpavedSurfaceTags),
};

const advRules = {
  highway: {
    primary: priority,
    secondary: priority,
    tertiary: priority,
    unclassified: priority,
    track: avoid,
    path: avoid,
  },
  surface: priorityRules(allSurfaceTags),
  smoothness: priorityRules(allSmoothnessTags),
};

const touringRules = {
  highway: {
    primary: priority,
    secondary: priority,
    tertiary: priority,
    unclassified: priority,
    track: avoid,
    path: avoid,
  },
  surface: avoidRules(blockedTouringSurfaceTags),
};

export const DEFAULT_ROUTE_RULE_PRESET_ID: RouteRulePresetId = 'adv';

export const ROUTE_RULE_PRESETS: Record<RouteRulePresetId, { id: RouteRulePresetId; label: string; rules: Record<string, unknown> }> = {
  dualsport: { id: 'dualsport', label: 'Dualsport', rules: dualsportRules },
  adv: { id: 'adv', label: 'ADV', rules: advRules },
  touring: { id: 'touring', label: 'Touring', rules: touringRules },
};

export const ROUTE_RULE_PRESET_ORDER: RouteRulePresetId[] = ['dualsport', 'adv', 'touring'];

export function getRouteRulePreset(id: RouteRulePresetId) {
  return ROUTE_RULE_PRESETS[id];
}
