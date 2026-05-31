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
type RmdfTileStatus = 'remoteOnly' | 'downloaded' | 'queued' | 'downloading' | 'failed';

type RmdfTileViewModel = {
  id: string; // filename
  manifestTile: RmdfManifestTile;
  status: RmdfTileStatus;
  selected: boolean;
  localPath: string;
  queuePosition?: number;
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
3. When using a cached manifest because the network is disrupted, show a warning and allow delete-only mode. Do not allow starting new downloads in this state.
4. Download selected missing tiles from the CDN.
5. Limit concurrent downloads to avoid slow or unstable mobile behavior.
6. Show progress as count and bytes where available.
7. Warn before starting downloads on cellular data.
8. Continue downloads after the user leaves the RMDF download screen.
9. Show a global **Downloading map data** indicator at the top of every screen while downloads are active.
10. Show a **Downloading map data** system notification so downloads can continue if the user closes or backgrounds the app.

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
| Queued tile | primary/blue translucent fill with queue number label | primary/blue line |
| Downloading tile | primary fill with spinner/progress in overlay | primary line |
| Failed tile | red translucent fill | red line |

Selection behavior:

1. User taps the map.
2. Convert tap coordinate to tile by checking whether `lon/lat` is inside a tile `bounds`.
3. Toggle selection for that tile.
4. Recompute action button enabled states.
5. MVP supports individual tap selection only. No drag, lasso, or box selection.

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
    Download button with selected missing tile total size
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

const selectedMissingBytes = selectedMissingTiles.reduce((sum, tile) => sum + tile.size_bytes, 0);
const downloadButtonLabel = canDownload ? `Download ${formatBytes(selectedMissingBytes)}` : 'Download';
```

Actions:

- **Download** downloads only `selectedMissingTiles`.
- **Delete** deletes only `selectedDownloadedTiles`.
- If both downloaded and missing tiles are selected, both buttons are enabled.
- The **Download** button label shows the total size of selected missing tiles, formatted in MB or GB as appropriate.
- If downloads are already active, pressing **Download** adds selected missing tiles to the existing queue.
- If a queued tile is deleted before it starts downloading, remove it from the queue instead of deleting a file.
- Queued tiles must have a visual indication and show their queue number.

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
- Add new selected tiles to the active queue when a download batch is already running.
- Remove queued tiles when the user deletes them before they start downloading.
- Write temp files and atomically move into place.
- Verify downloaded size.
- Continue active download batches after leaving the RMDF download screen.
- Publish queue positions and global download state for the app-wide indicator.
- Publish notification state for background/closed-app download visibility.
- Detect cellular network usage and require user confirmation before starting.

### Tile Utilities

- Convert manifest tiles to GeoJSON.
- Hit-test a coordinate against manifest tile bounds.
- Format byte sizes and selected counts, using MB or GB depending on the selected download size.

## Background Download and Global Indicator

Downloads must not be tied to the RMDF screen component lifecycle.

Implementation approach:

1. Use Expo BackgroundTask for scheduling and OS-specific background execution behavior: `https://docs.expo.dev/versions/latest/sdk/background-task/`.
2. Add the required Expo background-task dependency and any companion task registration dependency required by the Expo API.
3. Create an app-level download manager/provider mounted near `apps/ridi-mobile/app/_layout.tsx`.
4. Store active jobs, queued tiles, queue positions, completed tiles, failed tiles, and byte/count progress in shared state.
5. Keep downloads running when the RMDF screen unmounts.
6. Render a global top overlay on every screen when a job is active: **Downloading map data** plus progress.
7. Use a system notification titled **Downloading map data** while downloads are active.
8. On notification tap, deep-link back to the RMDF download screen if possible.
9. If notification permission is denied or unavailable, warn that downloads may be interrupted, allow the download anyway, and hide/skip the notification.

Native/platform notes:

- Use Expo BackgroundTask as the primary abstraction for scheduling and OS constraints.
- Confirm the exact behavior on iOS and Android during implementation.
- If true closed-app continuation has platform limits, document them and keep the app behavior aligned with Expo BackgroundTask support.
- Notification permission denial must not block downloads. The app should warn once and continue without showing the notification.

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
- queued tile is deleted before download starts;
- delete fails;
- user leaves screen during active downloads;
- app is backgrounded or closed during active downloads;
- notification permission is denied or unavailable.

UI should show:

- loading state while fetching manifest;
- retry action for manifest failures;
- network-disrupted warning when running from cached manifest;
- per-batch progress during downloads;
- app-wide active download indicator;
- system notification for active downloads;
- cellular download warning;
- delete confirmation before removing files;
- summary after download/delete completes.

## Implementation Steps

1. Add `expo-file-system` to `apps/ridi-mobile/package.json` if needed.
2. Add Expo BackgroundTask and notification dependencies after confirming the exact Expo APIs to use.
3. Add RMDF TypeScript types and constants.
4. Add storage helpers for the RMDF directory.
5. Add manifest fetch/cache helpers, including cached-manifest delete-only mode.
6. Add tile download/delete helpers.
7. Add app-level download manager/provider.
8. Add queued tile state and queue position tracking.
9. Add global **Downloading map data** indicator.
10. Add active-download system notification.
11. Add manifest-to-GeoJSON and tile hit-test helpers.
12. Add `app/rmdf-download.tsx` with MapLibre map and overlays.
13. Add user-location camera initialization with manifest-center fallback.
14. Add the Settings screen button and route navigation.
15. Wire button enablement and selected tile actions.
16. Add delete confirmation, including queued-tile removal behavior.
17. Add cellular download warning.
18. Add basic validation and error states.
19. Confirm route generation uses the same RMDF directory.
20. Test on device/simulator with a small manifest and then the production-sized manifest.

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
15. Start a second download while another batch is active and confirm selected tiles are added to the current queue.
16. Confirm queued tiles show queue positions on the map.
17. Delete a queued tile and confirm it is removed from the queue before download starts.
18. Run with cached manifest after network failure and confirm only delete actions are available.
19. Select missing tiles and confirm the Download button shows the total selected download size in MB or GB.

Automated/unit tests, if the app test setup exists or is added:

- manifest parser rejects invalid data;
- tile bounds convert to valid GeoJSON polygons;
- hit-test returns the expected tile;
- button enablement splits selected downloaded and missing tiles correctly;
- download button label formats total selected missing tile size correctly in MB/GB;
- storage helpers ignore non-RMDF files.

## Sane Defaults

Use these defaults unless implementation reveals a strong reason to change them:

- Download concurrency: 3 tiles at a time.
- Byte formatting: show MB below 1 GB with 1 decimal place; show GB at 1 GB and above with 2 decimal places.
- Available tile border: muted gray.
- Downloaded tile: translucent green/blue fill.
- Selected missing tile: primary translucent fill.
- Selected downloaded tile: orange/red translucent fill.
- Queued tile: blue translucent fill with queue number label.
- Downloading tile: stronger primary fill plus progress in the bottom overlay/global indicator.
- Failed tile: red translucent fill.
- Notification permission denied wording: `Downloads may be interrupted because notifications are disabled. Downloads will continue while the app is active.`
- Cellular warning wording: `These map downloads may use mobile data. Continue?`
- Delete confirmation wording: `Delete {count} downloaded map tile(s)?`
- Network-disrupted warning wording: `Network connection is disrupted. Showing cached map data; downloads are unavailable until connection is restored.`


## Decisions and Open Questions

Decisions:

1. MVP downloaded-state validation uses file existence plus byte size. Checksums are not required.
2. Tile URL pattern is `https://maps.ridi.bike/rmdf/{filename}`.
3. Downloads should continue after leaving the RMDF screen.
4. Use Expo BackgroundTask for background scheduling and OS-specific behavior.
5. A global **Downloading map data** indicator should appear at the top of every screen while downloads are active.
6. A system notification titled **Downloading map data** should support ongoing downloads when the app is backgrounded or closed, subject to Expo BackgroundTask/platform constraints.
7. Delete requires confirmation.
8. Cellular downloads are allowed only after a warning/confirmation.
9. Default map camera uses user location first, then falls back to the approximate manifest center.
10. Tiles with `point_count: 0` and `line_count: 0` are still downloadable/selectable.
11. If the remote manifest fetch fails but a cached manifest exists, show a network-disrupted warning and allow delete-only mode.
12. MVP selection supports individual tile taps only.
13. Outdated local tile detection is out of scope for MVP.
14. If downloads are already active, newly selected tiles are added to the current queue.
15. Deleting a queued tile removes it from the queue before it starts downloading.
16. Queued tiles show a visual state and queue number.
17. Available disk-space warnings are skipped for MVP.
18. Download button shows the selected missing tile total size in MB or GB, whichever is more relevant.
19. If notification permission is denied, warn that downloads may be interrupted, allow downloads, and do not show a notification.
20. For unspecified UI details, use the sane defaults section.

Open questions:

None currently.
