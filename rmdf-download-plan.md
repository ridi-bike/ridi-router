# RMDF Data Download Plan

## Goal

Add an RMDF routing data download screen to `apps/ridi-mobile`.

Users should be able to:

1. Open Settings.
2. Tap **Routing data download**.
3. View a full-screen map.
4. See RMDF tile borders from `https://maps.ridi.bike/rmdf/manifest.json`.
5. Tap tiles to select or deselect them.
6. See which tiles are already downloaded.
7. Download selected missing tiles.
8. Delete selected downloaded tiles.

The remote manifest format is identical to `map-data/output/manifest.json`.

## Current App Context

- Navigation uses Expo Router.
- Existing tabs live under `apps/ridi-mobile/app/(tabs)/`.
- `settings.tsx` is currently an empty screen.
- The ride screen already uses `@maplibre/maplibre-react-native` and `ridiMapStyle`.
- `expo-file-system` is present in the lockfile, but it is not listed in `apps/ridi-mobile/package.json` dependencies.
- The native router module exposes `generateRouteJson(tilesDir, requestJson)`, so downloaded RMDF files need to live in a stable app-owned directory that can be passed as `tilesDir`.

## Proposed User Flow

```text
Settings tab
  -> Routing data download button
    -> RMDF download screen
      -> fetch manifest
      -> scan local RMDF directory
      -> draw tile grid on map
      -> user selects tiles
      -> Download and/or Delete actions
      -> refresh local downloaded state
```

## Data Model

### Manifest

```ts
type RmdfManifest = {
  version: string;
  tile_size_degrees: number;
  format_version: number;
  generated_at: string;
  source_files: string[];
  tiles: RmdfManifestTile[];
};

type RmdfManifestTile = {
  filename: string;
  col: number;
  row: number;
  bounds: {
    lat_min: number;
    lat_max: number;
    lon_min: number;
    lon_max: number;
  };
  neighbors: Record<string, string | null>;
  size_bytes: number;
  point_count: number;
  line_count: number;
  checksum: string;
};
```

### Local Tile State

```ts
type RmdfTileStatus = 'remoteOnly' | 'downloaded' | 'downloading' | 'failed';

type RmdfTileViewModel = {
  id: string; // filename
  manifestTile: RmdfManifestTile;
  status: RmdfTileStatus;
  selected: boolean;
  localPath: string;
};
```

### Storage Layout

Use a single RMDF directory in app-owned storage:

```text
<FileSystem.documentDirectory>/rmdf/
  manifest.json
  tile_1908_1439.rmdf
  tile_1909_1439.rmdf
  ...
```

Implementation notes:

- Save the latest downloaded manifest as `manifest.json` for offline display and consistency checks.
- Treat a tile as downloaded only if its file exists and its byte size matches `size_bytes` from the manifest.
- Checksum verification is not required for MVP.
- Use a temporary filename during download, then move into place after success.

```text
tile_1908_1439.rmdf.tmp
  -> verify size
  -> tile_1908_1439.rmdf
```

## Networking

Base URL:

```text
https://maps.ridi.bike/rmdf/
```

Manifest URL:

```text
https://maps.ridi.bike/rmdf/manifest.json
```

Tile URL pattern:

```text
https://maps.ridi.bike/rmdf/{filename}
```

Download behavior:

1. Fetch `manifest.json` on screen load.
2. If the network fetch fails, fall back to cached local `manifest.json` if present.
3. Download selected missing tiles from the CDN.
4. Limit concurrent downloads to avoid slow or unstable mobile behavior.
5. Show progress as count and bytes where available.
6. Warn before starting downloads on cellular data.
7. Continue downloads after the user leaves the RMDF download screen.
8. Show a global **Downloading map data** indicator at the top of every screen while downloads are active.
9. Show a **Downloading map data** system notification so downloads can continue if the user closes or backgrounds the app.

## Map Rendering

Use MapLibre on the new screen.

Tile geometry should be generated from `bounds`:

```ts
Polygon coordinates:
[
  [lon_min, lat_min],
  [lon_max, lat_min],
  [lon_max, lat_max],
  [lon_min, lat_max],
  [lon_min, lat_min],
]
```

Render as GeoJSON:

- one source for all manifest tile polygons;
- fill layer for downloaded tiles;
- fill layer for selected tiles;
- line layer for tile borders;
- optional symbol layer for labels at higher zooms.

Suggested styling:

| State | Fill | Border |
| --- | --- | --- |
| Available remote tile | transparent | muted line |
| Downloaded tile | green/blue translucent fill | stronger line |
| Selected missing tile | primary translucent fill | primary line |
| Selected downloaded tile | danger/orange translucent fill | danger/orange line |
| Downloading tile | primary fill with spinner/progress in overlay | primary line |
| Failed tile | red translucent fill | red line |

Selection behavior:

1. User taps the map.
2. Convert tap coordinate to tile by checking whether `lon/lat` is inside a tile `bounds`.
3. Toggle selection for that tile.
4. Recompute action button enabled states.

Performance notes:

- The manifest may contain thousands of tiles.
- Avoid rendering a separate React component per tile if MapLibre layers can handle it as one GeoJSON collection.
- Keep selected and downloaded tile ids in `Set<string>` and derive feature state/properties.

Default camera:

1. Prefer the user's current location when location permission is granted.
2. If location is unavailable or denied, center on the approximate center of all manifest tile bounds.
3. Use a zoom level that makes nearby/selectable tiles visible without requiring immediate zooming.

## Screen Layout

Route proposal:

```text
apps/ridi-mobile/app/rmdf-download.tsx
```

Navigation changes:

- Add a Stack screen in `apps/ridi-mobile/app/_layout.tsx` for `rmdf-download`.
- Add a Settings button in `apps/ridi-mobile/app/(tabs)/settings.tsx`:

```text
Routing data download
```

Screen layout:

```text
Full screen map
  top overlay:
    Back button
    title/status text
  bottom overlay:
    selected count
    downloaded count / total size
    Download button
    Delete button
global overlay on all app screens while active:
  Downloading map data indicator
```

Button enablement:

```ts
const selectedTiles = tiles.filter(tile => selectedIds.has(tile.filename));
const selectedMissingTiles = selectedTiles.filter(tile => !downloadedIds.has(tile.filename));
const selectedDownloadedTiles = selectedTiles.filter(tile => downloadedIds.has(tile.filename));

const canDownload = selectedMissingTiles.length > 0;
const canDelete = selectedDownloadedTiles.length > 0;
```

Actions:

- **Download** downloads only `selectedMissingTiles`.
- **Delete** deletes only `selectedDownloadedTiles`.
- If both downloaded and missing tiles are selected, both buttons are enabled.

Delete confirmation:

- Tapping **Delete** opens a confirmation dialog before files are removed.
- The confirmation should clearly show how many downloaded tiles will be deleted.

## File-System Service

Create a dedicated RMDF download/data service, for example:

```text
apps/ridi-mobile/src/rmdf/
  manifest.ts
  storage.ts
  downloads.ts
  tiles.ts
```

Responsibilities:

### Manifest Service

- Fetch remote manifest.
- Validate the shape enough to avoid crashing the UI.
- Read/write cached manifest.

### Storage Service

- Ensure RMDF directory exists.
- List local `.rmdf` files.
- Delete files.
- Build local tile paths.

### Download Service

- Download one tile.
- Download many selected tiles with a concurrency limit.
- Write temp files and atomically move into place.
- Verify downloaded size/checksum if practical.
- Continue active download batches after leaving the RMDF download screen.
- Publish global download state for the app-wide indicator.
- Publish notification state for background/closed-app download visibility.
- Detect cellular network usage and require user confirmation before starting.

### Tile Utilities

- Convert manifest tiles to GeoJSON.
- Hit-test a coordinate against manifest tile bounds.
- Format byte sizes and selected counts.

## Background Download and Global Indicator

Downloads must not be tied to the RMDF screen component lifecycle.

Implementation approach:

1. Create an app-level download manager/provider mounted near `apps/ridi-mobile/app/_layout.tsx`.
2. Store active jobs, queued tiles, completed tiles, failed tiles, and byte/count progress in shared state.
3. Keep downloads running when the RMDF screen unmounts.
4. Render a global top overlay on every screen when a job is active: **Downloading map data** plus progress.
5. Use a system notification titled **Downloading map data** while downloads are active.
6. On notification tap, deep-link back to the RMDF download screen if possible.

Native/platform notes:

- Add the required Expo notification/background dependencies if they are not already installed.
- Confirm iOS and Android constraints for long-running background downloads before implementation.
- If true closed-app continuation is not fully supported by Expo APIs alone, document the limitation and implement the closest supported behavior with resumable/background-capable downloads where available.

## Native Router Integration

The router already accepts `tilesDir`.

Follow-up work should ensure route generation uses the same RMDF directory:

```text
<FileSystem.documentDirectory>/rmdf/
```

If the existing route-generation UI or service does not yet choose a tiles directory, add a shared helper so routing and downloads agree on the same path.

## Error Handling

Handle these cases explicitly:

- manifest network fetch fails;
- local cached manifest is missing;
- manifest JSON is invalid;
- app storage directory cannot be created;
- tile download fails;
- downloaded tile size does not match manifest;
- delete fails;
- user leaves screen during active downloads;
- app is backgrounded or closed during active downloads;
- notification permission is denied or unavailable.

UI should show:

- loading state while fetching manifest;
- retry action for manifest failures;
- per-batch progress during downloads;
- app-wide active download indicator;
- system notification for active downloads;
- cellular download warning;
- delete confirmation before removing files;
- summary after download/delete completes.

## Implementation Steps

1. Add `expo-file-system` to `apps/ridi-mobile/package.json` if needed.
2. Add notification/background-download dependencies after confirming the Expo APIs to use.
3. Add RMDF TypeScript types and constants.
4. Add storage helpers for the RMDF directory.
5. Add manifest fetch/cache helpers.
6. Add tile download/delete helpers.
7. Add app-level download manager/provider.
8. Add global **Downloading map data** indicator.
9. Add active-download system notification.
10. Add manifest-to-GeoJSON and tile hit-test helpers.
11. Add `app/rmdf-download.tsx` with MapLibre map and overlays.
12. Add user-location camera initialization with manifest-center fallback.
13. Add the Settings screen button and route navigation.
14. Wire button enablement and selected tile actions.
15. Add delete confirmation.
16. Add cellular download warning.
17. Add basic validation and error states.
18. Confirm route generation uses the same RMDF directory.
19. Test on device/simulator with a small manifest and then the production-sized manifest.

## Testing Plan

Manual tests:

1. Open Settings and navigate to the RMDF download screen.
2. Confirm manifest fetch succeeds from the CDN.
3. Confirm tile borders render.
4. Tap a missing tile: Download button enables, Delete remains disabled.
5. Tap a downloaded tile: Delete button enables, Download remains disabled.
6. Select one missing and one downloaded tile: both buttons enable.
7. Download selected missing tiles.
8. Leave the screen and confirm downloads continue with a global indicator.
9. Background or close the app and confirm the download notification appears and behavior matches platform support.
10. Restart app and confirm downloaded state persists.
11. Delete selected downloaded tiles and confirm the delete dialog appears first.
12. Disable network and confirm cached manifest fallback works.
13. Deny location permission and confirm the map centers on manifest bounds.
14. Start a download on cellular and confirm the warning appears.

Automated/unit tests, if the app test setup exists or is added:

- manifest parser rejects invalid data;
- tile bounds convert to valid GeoJSON polygons;
- hit-test returns the expected tile;
- button enablement splits selected downloaded and missing tiles correctly;
- storage helpers ignore non-RMDF files.

## Decisions and Open Questions

Decisions:

1. MVP downloaded-state validation uses file existence plus byte size. Checksums are not required.
2. Tile URL pattern is `https://maps.ridi.bike/rmdf/{filename}`.
3. Downloads should continue after leaving the RMDF screen.
4. A global **Downloading map data** indicator should appear at the top of every screen while downloads are active.
5. A system notification titled **Downloading map data** should support ongoing downloads when the app is backgrounded or closed, subject to platform/API constraints.
6. Delete requires confirmation.
7. Cellular downloads are allowed only after a warning/confirmation.
8. Default map camera uses user location first, then falls back to the approximate manifest center.
9. Tiles with `point_count: 0` and `line_count: 0` are still downloadable/selectable.

Open questions:

1. Should the screen support offline use with only cached manifest data?
2. Should we allow selecting many tiles by dragging/lasso, or only tapping one tile at a time for MVP?
3. Should outdated local tiles be detected when the remote manifest version/checksum changes?
