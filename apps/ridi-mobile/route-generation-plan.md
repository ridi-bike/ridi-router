# Route Generation Plan

## Confirmed Decisions

- Route plans and generated routes must persist across app restarts.
- Persist route data in the synced TinyBase store.
- Add synced tables for `plans` and `routes`.
- A plan is one start/finish/rules set plus its generated route rows.
- If generation succeeds with zero routes, still save the plan with `route_count = 0`.
- The `All` rule uses the empty rule preset.
- Do not generate routes unless both start and finish points are covered by downloaded RMDF tiles.
- MVP tile validation only checks downloaded tile coverage for the start and finish coordinates. It does not check network connectivity or route corridor coverage.

## Goal

Wire up point-to-point route generation from the map:

1. User taps a map point.
2. Bottom drawer shows point details and route action buttons.
3. `Ride to` closes the bottom drawer and opens a top route-planning drawer.
4. The top drawer defaults `start` to current location and `finish` to the tapped point.
5. User chooses one of three routing rules.
6. `Go` calls the native Rust `ridi-router` bridge.
7. Generated routes are saved in synced TinyBase.
8. Generated routes are shown on the map.
9. The Data tab lists saved plans.
10. Selecting a Data row returns to the map and opens route review mode with a route slider.

## Current App Shape

```mermaid
flowchart TD
  MapTab[Map tab: app/(tabs)/index.tsx]
  DataTab[Data tab: app/(tabs)/data.tsx]
  RouterBridge[Native router module: modules/ridi-router]
  RmdfStorage[RMDF storage: src/rmdf/storage.ts]
  RmdfTiles[RMDF tile helpers: src/rmdf/tiles.ts]
  TinyBase[TinyBase stores: src/tinybase]
  SyncedStore[syncedStore]

  MapTab --> RouterBridge
  RouterBridge --> RmdfStorage
  MapTab --> RmdfTiles
  MapTab --> TinyBase
  DataTab --> TinyBase
  TinyBase --> SyncedStore
```

Relevant existing pieces:

- `app/(tabs)/index.tsx` already supports map taps, selected POI/place details, current location, and a bottom drawer.
- `modules/ridi-router/src/index.ts` exposes `generateRouteJson(tilesDir, requestJson)`.
- The native Rust bridge expects a JSON `RouteRequest` and returns `{ ok: true, value }` or `{ ok: false, error }`.
- `src/rmdf/storage.ts#getRmdfDirectory()` returns the absolute local RMDF directory expected by the native bridge.
- `src/rmdf/tiles.ts#hitTestTile()` can find the manifest tile containing a coordinate.
- `src/tinybase/synced/schema.ts` currently has no synced tables, so `plans` and `routes` can be added as the first synced domain tables.
- `app/(tabs)/data.tsx` is currently empty.

## Router API Shape

The native route request should be serialized like:

```json
{
  "mode": {
    "StartFinish": {
      "start": { "lat": 48.137, "lon": 11.576 },
      "finish": { "lat": 48.14, "lon": 11.58 }
    }
  },
  "rules": {}
}
```

The result value is a route computation containing routes with:

- `coords`: route coordinates as `[lat, lon]` pairs.
- `stats.len_m`: route length in meters.
- `stats.score`: router score.
- `stats.junction_count`.
- `stats.highway`, `stats.surface`, `stats.smoothness`: stat maps with `len_m` and `percentage`.
- `stats.cluster`.
- `stats.approximated_route`.

## Synced TinyBase Data Model

Use synced tables because route plans are user-owned saved data.

### `plans`

One row per generated or attempted route plan.

Row ID:

- Use a `ksuid` because there is no stable external domain ID.

Cells:

```ts
plans: {
  plan_id: string;
  created_at: string;
  updated_at?: string;
  deleted_at?: string;
  schema_version: number;

  start_label: string;
  start_latitude: number;
  start_longitude: number;
  start_source: 'current_location' | 'map_selection';
  start_feature_kind?: 'coordinate' | 'poi' | 'name';
  start_tile_id: string;

  finish_label: string;
  finish_latitude: number;
  finish_longitude: number;
  finish_source: 'current_location' | 'map_selection';
  finish_feature_kind?: 'coordinate' | 'poi' | 'name';
  finish_tile_id: string;

  rule_preset_id: 'prefer_unpaved' | 'all' | 'avoid_unpaved';
  rule_label: string;
  rules_json: string;

  route_count: number;
  total_distance_m: number;
  selected_route_index: number;

  generation_status: 'succeeded' | 'failed';
  error_code?: string;
  error_message?: string;
  error_details?: string;
}
```

Notes:

- Use stable `snake_case` cell names.
- Store dates as ISO strings.
- Use soft deletes via `deleted_at`.
- For zero-route success, save `generation_status = 'succeeded'`, `route_count = 0`, and `total_distance_m = 0`.
- If the native router returns an error, save the plan with `generation_status = 'failed'` and no route rows only if the user had a valid start/finish/rules set.

### `routes`

One row per generated route for a plan.

Row ID:

- `${plan_id}:${route_index}`

Cells:

```ts
routes: {
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
}
```

Notes:

- Store raw route geometry as JSON strings for MVP simplicity.
- `coords_json` stores router `[lat, lon]` pairs.
- Map rendering converts `[lat, lon]` to GeoJSON `[lon, lat]`.
- `details_json` can hold a normalized copy of any extra router details not promoted to first-class cells.

## Route Domain Types

Define shared route point and plan types in `src/routes/types.ts`:

```ts
type RoutePoint = {
  latitude: number;
  longitude: number;
  label: string;
  source: 'current_location' | 'map_selection';
  featureKind?: 'coordinate' | 'poi' | 'name';
  tileId?: string;
};
```

Label rules:

1. Prefer selected feature name when available.
2. Use `Current location` for default start.
3. Fall back to GPS coordinates formatted as `lat, lon`.

## Map UI Modes

Use explicit map UI modes instead of many independent booleans:

```ts
type MapRouteMode =
  | { kind: 'idle' }
  | { kind: 'point_selected'; selectedPoint: SelectedPoint }
  | { kind: 'planning'; draft: RouteDraft }
  | { kind: 'generating'; draft: RouteDraft }
  | { kind: 'route_review'; planId: string; selectedRouteIndex: number };
```

This prevents bottom drawer, top drawer, generation state, and route review from fighting each other.

## Rule Presets

Add route rule presets in app code:

- `prefer_unpaved`: from `rule-examples/rules-prefer-unpaved.json`
- `all`: empty rule preset
- `avoid_unpaved`: from `rule-examples/rules-avoid-unpaved.json`

UI labels:

- `Prefer unpaved`
- `All`
- `Avoid unpaved`

Implementation location:

- `src/routes/rulePresets.ts`

Important note:

- I only found `rules-default.json`, `rules-prefer-unpaved.json`, and `rules-avoid-unpaved.json` in the local repo.
- If an explicit empty example file exists elsewhere, copy that exact object.
- Otherwise use the empty object `{}` for `all` and verify the Rust request deserializer accepts it. If it does not, create a full default `RouterRules` object with all rule fields empty/defaulted.

## Tile Coverage Guard

Do not let the user create a route generation request unless both start and finish are inside downloaded RMDF tiles.

Use:

- `readCachedManifest()` or existing manifest state for tile bounds.
- `hitTestTile(tiles, longitude, latitude)` to find the containing tile.
- `useRmdfDownloadedTileIds()` or TinyBase `rmdf_tiles.status === 'downloaded'` to confirm the tile file exists locally.

Validation rules:

1. When a user taps a map point, find its tile.
2. If no tile exists in the manifest or the tile is not downloaded, show a warning toast: `No downloaded routing tile exists for this point.`
3. Do not show `Ride to` or `Ride from` for a point without a downloaded tile.
4. When defaulting `start` to current location, also validate its tile.
5. If current location has no downloaded tile, leave `start` empty and show a warning if the user tries to generate.
6. On `Go`, revalidate both start and finish tiles before calling Rust.
7. Save `start_tile_id` and `finish_tile_id` into the `plans` row.

MVP limitation:

- Only start and finish coverage are checked.
- Do not check route corridor coverage or tile-to-tile graph connectivity.
- If both endpoints have tiles but the router cannot find a route, save a zero-route plan.

## Map Planning Visuals

When both start and finish are selected:

- Fit/zoom the camera to show both points.
- Add a start marker.
- Add a finish marker.
- Use green iconography for both markers, but make them distinct:
  - Start: green circular marker with a play/arrow glyph.
  - Finish: green marker with a flag/checkered flag glyph.
- Draw a straight dashed guide line between start and finish.
- Add arrows along the dashed line to indicate travel direction from start to finish.

The guide line is only a planning aid. It is not a generated route.

Suggested GeoJSON:

```ts
{
  type: 'Feature',
  properties: { kind: 'planning_direction' },
  geometry: {
    type: 'LineString',
    coordinates: [
      [start.longitude, start.latitude],
      [finish.longitude, finish.latitude]
    ]
  }
}
```

## Generated Route Map Rendering

Add route display layers to the MapLibre map:

- Use a GeoJSON source for generated routes.
- Convert router `[lat, lon]` pairs to GeoJSON `[lon, lat]` coordinates.
- Render non-selected routes with a muted stroke.
- Render selected route with a stronger stroke and higher z-order.
- Fit camera to generated route bounds after generation or Data row selection.
- For zero-route plans, show the start/finish markers and planning guide line, plus a `No routes found` message in the top drawer.

Route source shape:

```ts
{
  type: 'FeatureCollection',
  features: routes.map((route, index) => ({
    type: 'Feature',
    properties: { index, selected: index === selectedRouteIndex },
    geometry: {
      type: 'LineString',
      coordinates: route.coords.map(([lat, lon]) => [lon, lat])
    }
  }))
}
```

## Shared Route Generation Module

Create:

```text
src/routes/
  types.ts
  rulePresets.ts
  format.ts
  requests.ts
  persistence.ts
  tileCoverage.ts
  RouteGenerationProvider.tsx
  hooks.ts
```

Responsibilities:

- Hold the current route draft.
- Hold global `isGenerating` state.
- Prevent duplicate `Go` presses while generating.
- Validate downloaded tile coverage for start and finish.
- Call `generateRouteJson(getRmdfDirectory(), requestJson)`.
- Normalize native results into app route objects.
- Persist `plans` and `routes` rows in `syncedStore`.
- Read persisted plans/routes for the Data tab and map review.
- Expose selected plan and selected route index.

## Drawer Behavior

### Bottom Drawer: Point Details

When a downloaded-tile point is selected:

- Continue showing title, coordinates, classification, and details.
- Add route action buttons near the top or bottom.

Button rules:

- Always show `Ride to` if the selected point has a downloaded tile.
- Show `Ride from` only when the route draft exists and `start` is empty.
- `Ride to` sets `finish` from the selected point, closes bottom drawer, opens top route drawer.
- `Ride from` sets `start` from the selected point and opens/updates planning mode.

When a point lacks a downloaded tile:

- Show a warning toast.
- Keep normal point details if desired.
- Hide or disable route action buttons.

### Top Drawer: Route Planning

Fields:

- `Start`
- `Finish`

Each field shows:

- Name if available.
- GPS coordinates otherwise.
- `×` button on the right to clear the field.

Defaults:

- `start`: current location, if available and covered by a downloaded tile.
- `finish`: tapped point after `Ride to`.

Rules row:

- `Prefer unpaved`
- `All`
- `Avoid unpaved`

Actions:

- `Go`: enabled only when start and finish are present, both have downloaded tiles, and generation is not running.
- While generation runs, show disabled/loading state.

### Top Drawer: Route Review

When viewing a saved plan:

- Show route overview:
  - start
  - finish
  - rule name
  - generated route count
  - total distance
- Show `No routes found` if `route_count = 0`.
- Show `×` to close route view.
- Closing route view clears selected map review state but does not delete the saved plan.

### Bottom Slider: Route Review

Use a horizontal paged list at the bottom:

- One card per generated route.
- Swiping updates `selected_route_index` in `plans` and updates map styling.
- Hide the slider for zero-route plans.

Card details:

- Route number.
- Length.
- Score.
- Junction count.
- Top highway/surface/smoothness stats, where useful.
- Cluster if available.

## Global Generation Notification

Show a global in-app notification while route generation is running.

Suggested implementation:

- Put `RouteGenerationProvider` above the tab navigator.
- Add a `RouteGenerationBanner` in `app/_layout.tsx` or just inside the root layout.
- Banner reads `isGenerating` from route generation context.

Text:

- `Generating routes…`

Behavior:

- Visible on all tabs/screens.
- Does not block navigation.
- `Go` remains disabled while visible.

## Data Tab

Populate `app/(tabs)/data.tsx` from synced TinyBase `plans`.

Each row should show:

- Start label.
- Finish label.
- Rule name.
- Generated route count.
- Total distance.
- Generation status.
- Created time, optional.

On row press:

1. Select the plan in shared route state.
2. Set selected route index to `plans.selected_route_index` or `0`.
3. Navigate to the map tab.
4. Map tab enters `route_review` mode.
5. Camera fits generated route bounds if routes exist; otherwise it fits start and finish.

## Route Generation Flow

1. Map press finds a feature and selected coordinates.
2. App checks whether the point is inside a downloaded RMDF tile.
3. If no downloaded tile exists, show warning toast and block route actions.
4. Bottom drawer opens with point details for valid route points.
5. User taps `Ride to`.
6. App creates/updates route draft:
   - `start`: current location, if available and covered by downloaded tile.
   - `finish`: tapped point.
   - `rulePresetId`: default `all`.
7. Bottom drawer closes.
8. Top planning drawer opens.
9. If both start and finish exist, map fits both and shows green start/finish markers plus dashed directional guide line.
10. User optionally clears/replaces fields and changes rules.
11. User taps `Go`.
12. App revalidates start, finish, downloaded tiles, and generation idle state.
13. App sets `isGenerating = true`.
14. App builds `RouteRequest` JSON.
15. App calls native `generateRouteJson`.
16. App saves a `plans` row for the attempt.
17. If generation succeeded, app saves zero or more `routes` rows.
18. Data tab updates from synced TinyBase.
19. Map enters route review mode and shows generated routes, or a zero-route overview.
20. `isGenerating = false`.

## Error Handling

Handle these cases explicitly:

- No current location permission: leave `start` empty and require user to choose `Ride from`.
- Current location has no downloaded tile: leave `start` empty and show a tile warning when needed.
- Tapped point has no downloaded tile: show warning toast and block route actions.
- User clears start/finish: disable `Go` until both exist.
- RMDF directory missing `manifest.json`: show a clear error and link user toward RMDF download.
- Native router returns error envelope: save failed plan metadata and display `error.message`; optionally display `error.details` in development.
- Native router succeeds with no routes: save succeeded plan with `route_count = 0` and no `routes` rows.
- App navigates away during generation: banner remains visible and result still lands in synced TinyBase.

## Implementation Phases

### Phase 1: Synced Schema

- Add Zod schemas for `plans` and `routes` to `src/tinybase/synced/schema.ts`.
- Export typed helpers/hooks for plans and routes.
- Use additive synced schema practices from `AGENTS.md`.
- Use soft deletes via `deleted_at`.

### Phase 2: Route Domain and Persistence

- Add route domain types.
- Add rule presets.
- Add request builder for Rust `RouteRequest`.
- Add result normalizer and format helpers.
- Add TinyBase persistence helpers for plan and route rows.

### Phase 3: Tile Coverage Guard

- Add `src/routes/tileCoverage.ts`.
- Use manifest tile hit testing and downloaded tile IDs.
- Block `Ride to`, `Ride from`, and `Go` when endpoint tiles are missing.
- Add warning toast for missing endpoint tiles.

### Phase 4: Map Planning UI

- Refactor `app/(tabs)/index.tsx` from `selectedPoint`-only state to explicit map route modes.
- Add `Ride to` and conditional `Ride from` buttons.
- Add top planning drawer.
- Add clear buttons for start/finish.
- Add rules selector.
- Add green start/finish markers.
- Add dashed directional guide line.
- Fit camera to start/finish when both are set.
- Add disabled/loading `Go` behavior.

### Phase 5: Native Generation Integration

- Call `generateRouteJson` with `getRmdfDirectory()`.
- Validate and normalize native result.
- Save plan and routes to synced TinyBase.
- Add global generation banner.
- Add error display.

### Phase 6: Generated Route Rendering

- Add GeoJSON route source/layers.
- Show all routes and selected route.
- Fit camera to generated route bounds.
- Add route review top drawer and close behavior.
- Add bottom route slider.
- Handle zero-route saved plans.

### Phase 7: Data Tab

- Render synced `plans` rows.
- Navigate from row press back to map route review.
- Keep selected route index in synced `plans.selected_route_index`.

## Testing Checklist

Manual checks:

- Tap named POI with downloaded tile: `finish` uses name.
- Tap unnamed place with downloaded tile: `finish` uses coordinates.
- Tap a point without a downloaded tile: warning toast appears and route buttons are blocked.
- `Ride to` opens top drawer and closes bottom drawer.
- `start` defaults to current location only when its tile is downloaded.
- Clearing `start` makes `Ride from` appear in bottom drawer for valid points.
- Clearing `finish` requires `Ride to` before generation.
- When start and finish exist, map fits both.
- Start and finish markers are green and distinct.
- Dashed directional guide line points from start to finish.
- Rule buttons update selected rule.
- `All` sends the empty rule preset.
- `Go` is disabled while generating.
- Global generation banner appears on map, data, and settings tabs.
- Generated routes persist in synced TinyBase.
- Zero-route generation persists a plan with no route rows.
- Data tab gets one row per saved plan.
- Pressing Data row returns to map and opens review UI.
- Swiping route cards changes selected route line and updates selected route index.
- Closing review hides route review drawer without deleting the saved plan.

Automated checks:

- Synced schemas accept valid `plans` and `routes` rows.
- Request builder emits expected `StartFinish` JSON.
- Empty `all` preset is serialized as expected.
- Rule preset IDs map to expected rule objects.
- Tile coverage helper rejects missing or undownloaded endpoint tiles.
- Coordinate conversion from `[lat, lon]` to `[lon, lat]` is correct.
- Result normalizer handles empty route arrays and missing optional stats.
- Persistence helper writes one plan and N routes.
- Persistence helper writes one plan and zero routes for zero-route success.
- `Go` guard prevents concurrent native calls.

## Remaining Questions

1. Where is the exact “empty example rule file”? I only found `rules-default.json`, `rules-prefer-unpaved.json`, and `rules-avoid-unpaved.json` locally.
2. What toast system should we use for warnings? There is no obvious toast component in the app yet.
3. Should failed native generation attempts be saved as failed plans, or only successful/zero-route plans?
4. Should the Data tab include soft-deleted plans behind a filter, or hide them always for MVP?
