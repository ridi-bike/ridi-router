# Route Generation Plan

## Goal

Wire up point-to-point route generation from the map:

1. User taps a map point.
2. Bottom drawer shows point details and route action buttons.
3. `Ride to` closes the bottom drawer and opens a top route-planning drawer.
4. The top drawer defaults `start` to current location and `finish` to the tapped point.
5. User chooses one of three routing rules.
6. `Go` calls the native Rust `ridi-router` bridge.
7. Generated routes are shown on the map.
8. The Data tab lists generated route summaries.
9. Selecting a Data row returns to the map and opens route review mode with a route slider.

## Current App Shape

```mermaid
flowchart TD
  MapTab[Map tab: app/(tabs)/index.tsx]
  DataTab[Data tab: app/(tabs)/data.tsx]
  RouterBridge[Native router module: modules/ridi-router]
  RmdfStorage[RMDF storage: src/rmdf/storage.ts]
  TinyBase[TinyBase stores: src/tinybase]

  MapTab --> RouterBridge
  RouterBridge --> RmdfStorage
  MapTab --> TinyBase
  DataTab --> TinyBase
```

Relevant existing pieces:

- `app/(tabs)/index.tsx` already supports map taps, selected POI/place details, current location, and a bottom drawer.
- `modules/ridi-router/src/index.ts` exposes `generateRouteJson(tilesDir, requestJson)`.
- The native Rust bridge expects a JSON `RouteRequest` and returns `{ ok: true, value }` or `{ ok: false, error }`.
- `src/rmdf/storage.ts#getRmdfDirectory()` returns the absolute local RMDF directory expected by the native bridge.
- `app/(tabs)/data.tsx` is currently empty.
- TinyBase is already split into `localStore` and `syncedStore`.

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

## UX State Model

### Route Points

Define a shared route point type:

```ts
type RoutePoint = {
  latitude: number;
  longitude: number;
  label: string;
  source: 'current_location' | 'map_selection';
  featureKind?: 'coordinate' | 'poi' | 'name';
};
```

Label rules:

1. Prefer selected feature name when available.
2. Use `Current location` for default start.
3. Fall back to GPS coordinates formatted as `lat, lon`.

### Map Modes

Use explicit map UI mode instead of many independent booleans:

```ts
type MapRouteMode =
  | { kind: 'idle' }
  | { kind: 'point_selected'; selectedPoint: SelectedPoint }
  | { kind: 'planning'; draft: RouteDraft }
  | { kind: 'generating'; draft: RouteDraft }
  | { kind: 'route_review'; generationId: string; selectedRouteIndex: number };
```

This prevents bottom drawer, top drawer, generation state, and route review from fighting each other.

## Rule Presets

Add route rule presets in app code, copied from the repository examples:

- `prefer_unpaved`: from `rule-examples/rules-prefer-unpaved.json`
- `all`: default router rules / neutral rules
- `avoid_unpaved`: from `rule-examples/rules-avoid-unpaved.json`

UI labels:

- `Prefer unpaved`
- `All`
- `Avoid unpaved`

Implementation location:

- `src/routes/rulePresets.ts`

The `all` preset needs confirmation. It can be either:

1. `{}` if Rust serde defaults fill missing `RouterRules` fields correctly, or
2. the full default rules object from `rule-examples/rules-default.json`.

## Shared Route Generation State

Create a small route generation module:

```text
src/routes/
  types.ts
  rulePresets.ts
  format.ts
  requests.ts
  RouteGenerationProvider.tsx
  hooks.ts
```

Responsibilities:

- Hold the current draft route.
- Hold global `isGenerating` state.
- Prevent duplicate `Go` presses while generating.
- Call `generateRouteJson(getRmdfDirectory(), requestJson)`.
- Normalize native results into app route objects.
- Expose generated route sessions to the map and Data tab.
- Expose selected generation and selected route index.

Provider state is enough for cross-tab interaction during one app session. If route history should survive app restart, persist route generations in TinyBase.

## Persistence Decision

There are two valid designs:

### Option A: Session-only route generations

Use React context only.

Pros:

- Fastest implementation.
- No schema migration.
- Avoids storing large route coordinate arrays.

Cons:

- Data tab clears after app restart.
- Generated routes are not future-syncable.

### Option B: Persist route generations

Use TinyBase for route generation records.

Recommended split if persistence is required:

- `syncedStore`: route generation metadata if generated routes are considered user-owned history.
- `localStore`: raw generated route geometry/cache if considered derived local operational data.

Possible schema:

```ts
route_generations: {
  generation_id: string;
  created_at: string;
  start_label: string;
  start_latitude: number;
  start_longitude: number;
  finish_label: string;
  finish_latitude: number;
  finish_longitude: number;
  rule_preset_id: string;
  rule_label: string;
  route_count: number;
  total_distance_m: number;
  selected_route_index: number;
  deleted_at?: string;
}

route_generation_results: {
  generation_id: string;
  routes_json: string;
  generated_at: string;
}
```

This needs a final product decision because synced schemas must be additive after release.

## Map Rendering

Add route display layers to the MapLibre map:

- Use a GeoJSON source for all generated routes.
- Convert router `[lat, lon]` pairs to GeoJSON `[lon, lat]` coordinates.
- Render non-selected routes with a muted stroke.
- Render selected route with a stronger stroke and higher z-order.
- Fit camera to generated route bounds after generation or Data row selection.

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

## Drawer Behavior

### Bottom Drawer: Point Details

When a point is selected:

- Continue showing title, coordinates, classification, and details.
- Add route action buttons near the top or bottom.

Button rules:

- Always show `Ride to`.
- Show `Ride from` only when the route draft exists and `start` is empty.
- `Ride to` sets `finish` from the selected point, closes bottom drawer, opens top route drawer.
- `Ride from` sets `start` from the selected point and keeps/opens planning mode.

### Top Drawer: Route Planning

Fields:

- `Start`
- `Finish`

Each field shows:

- Name if available.
- GPS coordinates otherwise.
- `×` button on the right to clear the field.

Defaults:

- `start`: current location, if available.
- `finish`: tapped point after `Ride to`.

Rules row:

- `Prefer unpaved`
- `All`
- `Avoid unpaved`

Actions:

- `Go`: enabled only when start and finish are both present and generation is not running.
- While generation runs, show disabled/loading state.

### Top Drawer: Route Review

When viewing a generated result:

- Show route overview:
  - start
  - finish
  - rule name
  - generated route count
  - total or selected route distance
- Show `×` to close route view.
- Closing route view clears selected generation from map review mode, but should not delete the Data tab row.

### Bottom Slider: Route Review

Use a horizontal paged list at the bottom:

- One card per generated route.
- Swiping updates `selectedRouteIndex`.
- Map updates selected route styling.

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

Populate `app/(tabs)/data.tsx` with generated route sessions.

Each row should show:

- Start label.
- Finish label.
- Rule name.
- Generated route count.
- Distance.
- Created time, optional.

On row press:

1. Select the generation in shared route state.
2. Set selected route index to `0` or the last selected index.
3. Navigate to the map tab.
4. Map tab enters `route_review` mode.
5. Camera fits the selected/all route bounds.

## Route Generation Flow

1. Map press finds a feature and selected coordinates.
2. Bottom drawer opens with point details.
3. User taps `Ride to`.
4. App creates/updates route draft:
   - `start`: current location, if available.
   - `finish`: tapped point.
   - `rulePresetId`: default, likely `all`.
5. Bottom drawer closes.
6. Top planning drawer opens.
7. User optionally clears/replaces fields and changes rules.
8. User taps `Go`.
9. App validates start, finish, RMDF manifest, and generation idle state.
10. App sets `isGenerating = true`.
11. App builds `RouteRequest` JSON.
12. App calls native `generateRouteJson`.
13. App normalizes success or displays error.
14. App stores the generation result.
15. Data tab updates.
16. Map enters route review mode and shows routes.
17. `isGenerating = false`.

## Error Handling

Handle these cases explicitly:

- No current location permission: allow empty start and require user to choose `Ride from`.
- User clears start/finish: disable `Go` until both exist.
- RMDF directory missing `manifest.json`: show a clear error and link user toward RMDF download.
- Native router returns error envelope: display `error.message` and optionally `error.details` in development.
- No routes returned: add a Data row only if desired; otherwise show `No routes found`.
- App navigates away during generation: banner remains visible and result still lands in route state.

## Implementation Phases

### Phase 1: Types and Route Service

- Add route domain types.
- Add rule presets.
- Add request builder for Rust `RouteRequest`.
- Add result normalizer and format helpers.
- Add provider/hook for route generation state.

### Phase 2: Map Planning UI

- Refactor `app/(tabs)/index.tsx` from `selectedPoint`-only state to explicit map route modes.
- Add `Ride to` and conditional `Ride from` buttons.
- Add top planning drawer.
- Add clear buttons for start/finish.
- Add rules selector.
- Add disabled/loading `Go` behavior.

### Phase 3: Native Generation Integration

- Call `generateRouteJson` with `getRmdfDirectory()`.
- Validate and normalize native result.
- Add global generation banner.
- Add error display.

### Phase 4: Map Route Rendering

- Add GeoJSON route source/layers.
- Show all routes and selected route.
- Fit camera to generated route bounds.
- Add route review top drawer and close behavior.
- Add bottom route slider.

### Phase 5: Data Tab

- Render generation rows.
- Navigate from row press back to map route review.
- Keep selected route index in shared state.

### Phase 6: Persistence, If Needed

- Add TinyBase schemas only after deciding persistence rules.
- If using `syncedStore`, keep changes additive and use soft deletes.
- If using `localStore`, keep only operational/derived data there.

## Testing Checklist

Manual checks:

- Tap named POI: `finish` uses name.
- Tap unnamed place: `finish` uses coordinates.
- `Ride to` opens top drawer and closes bottom drawer.
- `start` defaults to current location.
- Clearing `start` makes `Ride from` appear in bottom drawer.
- Clearing `finish` requires `Ride to` before generation.
- Rule buttons update selected rule.
- `Go` is disabled while generating.
- Global generation banner appears on map, data, and settings tabs.
- Generated routes render on map.
- Data tab gets one row per generated session.
- Pressing Data row returns to map and opens review UI.
- Swiping route cards changes selected route line.
- Closing review hides route review drawer without deleting Data row.

Automated checks:

- Request builder emits expected `StartFinish` JSON.
- Rule preset IDs map to expected rule objects.
- Coordinate conversion from `[lat, lon]` to `[lon, lat]` is correct.
- Result normalizer handles empty route arrays and missing optional stats.
- `Go` guard prevents concurrent native calls.

## Open Questions

1. Should generated route history survive app restart, or is session-only enough for now?
2. If route history persists, should route metadata be considered user-owned synced data in `syncedStore`?
3. For the `All` rule, should we use empty/default Rust rules or copy `rule-examples/rules-default.json`?
4. What should happen when generation returns zero routes: create a Data row or only show an error/empty notice?
5. Should the Data tab list every generation, or only the most recent generated route set?
6. Should tapping `Ride from` immediately reopen the top planning drawer, or only fill start and leave the bottom drawer visible?
7. Should the route review show total distance across all generated routes, or the selected route distance as the main distance?
8. Do we need a cancel button while generation is running, or only disable `Go` until completion?
9. Should selected route styling show all alternate routes or only the selected one?
10. Should route generation automatically select/download missing RMDF tiles, or fail with a download prompt when tiles are missing?
