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
- Treat a tile as downloaded only if its file exists and, ideally, matches the manifest checksum.
- Use a temporary filename during download, then move into place after success.

```text
tile_1908_1439.rmdf.tmp
  -> verify size/checksum
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

### Tile Utilities

- Convert manifest tiles to GeoJSON.
- Hit-test a coordinate against manifest tile bounds.
- Format byte sizes and selected counts.

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
- downloaded tile size/checksum does not match manifest;
- delete fails;
- user leaves screen during active downloads.

UI should show:

- loading state while fetching manifest;
- retry action for manifest failures;
- per-batch progress during downloads;
- summary after download/delete completes.

## Implementation Steps

1. Add `expo-file-system` to `apps/ridi-mobile/package.json` if needed.
2. Add RMDF TypeScript types and constants.
3. Add storage helpers for the RMDF directory.
4. Add manifest fetch/cache helpers.
5. Add tile download/delete helpers.
6. Add manifest-to-GeoJSON and tile hit-test helpers.
7. Add `app/rmdf-download.tsx` with MapLibre map and overlays.
8. Add the Settings screen button and route navigation.
9. Wire button enablement and selected tile actions.
10. Add basic validation and error states.
11. Confirm route generation uses the same RMDF directory.
12. Test on device/simulator with a small manifest and then the production-sized manifest.

## Testing Plan

Manual tests:

1. Open Settings and navigate to the RMDF download screen.
2. Confirm manifest fetch succeeds from the CDN.
3. Confirm tile borders render.
4. Tap a missing tile: Download button enables, Delete remains disabled.
5. Tap a downloaded tile: Delete button enables, Download remains disabled.
6. Select one missing and one downloaded tile: both buttons enable.
7. Download selected missing tiles.
8. Restart app and confirm downloaded state persists.
9. Delete selected downloaded tiles.
10. Disable network and confirm cached manifest fallback works.

Automated/unit tests, if the app test setup exists or is added:

- manifest parser rejects invalid data;
- tile bounds convert to valid GeoJSON polygons;
- hit-test returns the expected tile;
- button enablement splits selected downloaded and missing tiles correctly;
- storage helpers ignore non-RMDF files.

## Open Questions

1. Should checksum verification be required for MVP, or is file existence plus size enough?
2. Should the screen support offline use with only cached manifest data?
3. Should downloads continue in the background after leaving the screen, or cancel/pause?
4. Should we allow selecting many tiles by dragging/lasso, or only tapping one tile at a time for MVP?
5. Should there be a confirmation dialog before deleting downloaded routing data?
6. Should the app prevent downloading on cellular, warn first, or allow it?
7. What should the default map camera be: user location, all manifest bounds, or a fixed region?
8. Should empty tiles with `point_count: 0` and `line_count: 0` be downloadable/selectable?
9. Should outdated local tiles be detected when the remote manifest version/checksum changes?
10. Is `https://maps.ridi.bike/rmdf/{filename}` the final tile URL pattern?
