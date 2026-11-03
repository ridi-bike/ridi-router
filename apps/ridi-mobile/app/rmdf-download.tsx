import { Camera, GeoJSONSource, Images, Layer, Map, type MapProps } from '@maplibre/maplibre-react-native';
import * as Location from 'expo-location';
import { router } from 'expo-router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { ridiMapStyle } from '@/map/ridiMapStyle';
import { CELLULAR_WARNING } from '@/rmdf/constants';
import { isCellularConnection } from '@/rmdf/downloads';
import { loadRmdfChunk, loadRmdfRelease } from '@/rmdf/manifest';
import { useRmdfDownloadManager } from '@/rmdf/RmdfDownloadManagerContext';
import { useRmdfDownloads } from '@/rmdf/RmdfDownloadProvider';
import {
  boundsContain,
  boundsIntersect,
  boxesToGeoJson,
  boxLabelsToGeoJson,
  buildCellBoxes,
  buildChunkBoxes,
  buildTileBoxes,
  cellIdAt,
  chunkAt,
  chunkDrawBounds,
  chunkIdForTile,
  formatBytes,
  gridLevelForZoom,
  releaseCenter,
  tileIdAt,
  tilesInCell,
  tileStatesToGeoJson,
  tileStatus,
  type GridLevel,
} from '@/rmdf/tiles';
import type { RmdfBounds, RmdfChunkSummary, RmdfManifestTile, RmdfRelease } from '@/rmdf/types';
import { useLocalStore, useRmdfDownloadingTileIds, useRmdfQueuedTileIds, useRmdfTileTable } from '@/tinybase/hooks';
import { adoptUntrackedTiles, listUntrackedTileFiles, reconcileLocalRmdfTiles } from '@/tinybase/local/rmdfReconcile';
import { clearRmdfTileSelection, rowInstalledTileMeta, rowTileMeta, setRmdfTilesSelected } from '@/tinybase/local/rmdfTiles';

type LoadState =
  | { kind: 'loading' }
  | { kind: 'ready'; release: RmdfRelease; deleteOnly: boolean; warning?: string }
  | { kind: 'error'; message: string };

type ViewState = { zoom: number; bounds: RmdfBounds };

const INITIAL_ZOOM = 6;
// Chunk manifests to fetch for the viewport at once; more means the view is zoomed out too far.
const MAX_VISIBLE_CHUNK_LOADS = 6;

const TILE_COLORS = {
  downloaded: '#22c55e',
  outdated: '#f59e0b',
  pending: '#93c5fd',
  failed: '#ef4444',
  selected: '#208AEF',
} as const;

const tileFillStyle = {
  fillColor: ['match', ['get', 'fill'], ...Object.entries(TILE_COLORS).flat(), 'rgba(0,0,0,0)'],
  fillOpacity: 0.45,
  fillOutlineColor: 'rgba(255,255,255,0)',
} as const;

const gridLineStyle = {
  lineColor: ['match', ['get', 'selection'], 'none', '#4b5563', '#208AEF'],
  lineWidth: ['match', ['get', 'selection'], 'all', 3, 'some', 2, ['match', ['get', 'level'], 'tile', 0.75, 1.25]],
} as const;

// Dash patterns cannot be data-driven, so partly selected squares get their own layer.
const SOLID_GRID_FILTER = ['!=', ['get', 'selection'], 'some'] as const;
const PARTLY_SELECTED_FILTER = ['==', ['get', 'selection'], 'some'] as const;
const partlySelectedLineStyle = { lineColor: '#208AEF', lineWidth: 2, lineDasharray: [2, 1] } as const;

const labelStyle = {
  textField: ['get', 'label'],
  textFont: ['Open Sans Regular'],
  textSize: 13,
  textColor: '#ffffff',
  textHaloColor: '#1f2937',
  textHaloWidth: 1.5,
  textAllowOverlap: false,
} as const;

const LEVEL_HINT: Record<GridLevel, string> = {
  chunk: 'Tap a square to select or deselect all of it. Zoom in for smaller squares.',
  cell: 'Tap a square to select or deselect all of it. Zoom in for single tiles.',
  tile: 'Tap a tile to select or deselect it.',
};

export default function RmdfDownloadScreen() {
  const insets = useSafeAreaInsets();
  const downloads = useRmdfDownloads();
  const localStore = useLocalStore();
  const rmdfDownloadManager = useRmdfDownloadManager();
  const rows = useRmdfTileTable();
  const queuedIds = useRmdfQueuedTileIds();
  const downloadingIds = useRmdfDownloadingTileIds();
  const [loadState, setLoadState] = useState<LoadState>({ kind: 'loading' });
  const [cameraCenter, setCameraCenter] = useState<[number, number] | null>(null);
  const [view, setView] = useState<ViewState | null>(null);
  const [chunkTiles, setChunkTiles] = useState<globalThis.Map<string, RmdfManifestTile[]>>(() => new globalThis.Map());
  const [chunkError, setChunkError] = useState<string | null>(null);
  const loadingChunks = useRef(new Set<string>());

  const release = loadState.kind === 'ready' ? loadState.release : null;

  const ensureChunks = useCallback(
    async (target: RmdfRelease, chunks: RmdfChunkSummary[]): Promise<RmdfManifestTile[]> => {
      const loaded = await Promise.all(
        chunks.map(async (chunk) => {
          const manifest = await loadRmdfChunk(target, chunk);
          if (!loadingChunks.current.has(chunk.id)) {
            loadingChunks.current.add(chunk.id);
            setChunkTiles((current) => new globalThis.Map(current).set(chunk.id, manifest.tiles));
          }
          return manifest.tiles;
        })
      );
      return loaded.flat();
    },
    []
  );

  const load = useCallback(async () => {
    setLoadState({ kind: 'loading' });
    try {
      const result = await loadRmdfRelease();
      await reconcileLocalRmdfTiles(localStore, result.release);
      loadingChunks.current.clear();
      setChunkTiles(new globalThis.Map());
      setLoadState({ kind: 'ready', release: result.release, deleteOnly: result.deleteOnly, warning: result.warning });
      setCameraCenter(await resolveInitialCamera(result.release));
      // Chunks holding tiles on disk are needed to tell which are outdated and to re-register
      // files the local store no longer lists.
      const chunkIds = new Set<string>();
      for (const row of Object.values(localStore.getTable('rmdf_tiles'))) {
        const meta = rowInstalledTileMeta(row);
        if (meta) chunkIds.add(chunkIdForTile(meta, result.release));
      }
      const untracked = await listUntrackedTileFiles(localStore);
      for (const filename of untracked) {
        const match = /^tile_(\d+)_(\d+)\.rmdf$/.exec(filename);
        if (match) chunkIds.add(chunkIdForTile({ col: Number(match[1]), row: Number(match[2]) }, result.release));
      }
      const tiles = await ensureChunks(result.release, result.release.chunks.filter((chunk) => chunkIds.has(chunk.id)));
      if (untracked.length > 0) await adoptUntrackedTiles(localStore, new globalThis.Map(tiles.map((tile) => [tile.filename, tile])));
    } catch (error) {
      setLoadState({ kind: 'error', message: error instanceof Error ? error.message : 'Failed to load map data index.' });
    }
  }, [ensureChunks, localStore]);

  useEffect(() => {
    const timeout = setTimeout(() => void load(), 0);
    return () => clearTimeout(timeout);
  }, [load]);

  const level: GridLevel = release && view ? gridLevelForZoom(view.zoom, release) : 'chunk';

  useEffect(() => {
    if (!release || !view || level === 'chunk') return;
    const visible = release.chunks.filter((chunk) => boundsIntersect(chunk.bounds, view.bounds));
    const missing = visible.filter((chunk) => !chunkTiles.has(chunk.id)).slice(0, MAX_VISIBLE_CHUNK_LOADS);
    if (missing.length === 0) return;
    ensureChunks(release, missing)
      .then(() => setChunkError(null))
      .catch((error: unknown) => setChunkError(error instanceof Error ? error.message : 'Failed to load map data.'));
  }, [release, view, level, chunkTiles, ensureChunks]);

  const loadedTiles = useMemo(() => {
    const byFilename = new globalThis.Map<string, RmdfManifestTile>();
    for (const tiles of chunkTiles.values()) for (const tile of tiles) byFilename.set(tile.filename, tile);
    return byFilename;
  }, [chunkTiles]);

  const tileState = useMemo(() => ({ rows, queuedIds, downloadingIds }), [rows, queuedIds, downloadingIds]);

  const boxes = useMemo(() => {
    if (!release) return [];
    if (level === 'chunk' || !view) return buildChunkBoxes(release, tileState);
    const tiles = [...loadedTiles.values()];
    return level === 'cell' ? buildCellBoxes(release, tiles, tileState, view.bounds) : buildTileBoxes(tiles, tileState, view.bounds);
  }, [release, level, view, loadedTiles, tileState]);

  const gridGeoJson = useMemo(() => boxesToGeoJson(boxes), [boxes]);
  const labelGeoJson = useMemo(() => boxLabelsToGeoJson(boxes), [boxes]);
  const tileGeoJson = useMemo(() => tileStatesToGeoJson(tileState, loadedTiles), [tileState, loadedTiles]);

  // Selection lives in tile rows, so it survives leaving the screen and spans chunks not loaded.
  const selection = useMemo(() => {
    const selected: { tile: RmdfManifestTile; installed: boolean; pending: boolean }[] = [];
    for (const [filename, row] of Object.entries(rows)) {
      if (row.selected !== true) continue;
      const tile = loadedTiles.get(filename) ?? rowTileMeta(row);
      if (!tile) continue;
      selected.push({ tile, installed: !!row.installed_meta, pending: queuedIds.has(filename) || downloadingIds.has(filename) });
    }
    const downloadable = selected.filter(({ tile, pending }) => !pending && rows[tile.filename]?.installed_checksum !== tile.checksum).map(({ tile }) => tile);
    const deletable = selected.filter(({ installed, pending }) => installed || pending).map(({ tile }) => tile);
    return {
      tiles: selected.map(({ tile }) => tile),
      downloadable,
      downloadableBytes: downloadable.reduce((sum, tile) => sum + tile.size_bytes, 0),
      deletable,
      deletableInstalledCount: selected.filter(({ installed }) => installed).length,
    };
  }, [rows, loadedTiles, queuedIds, downloadingIds]);

  const outdatedTiles = useMemo(
    () => [...loadedTiles.values()].filter((tile) => tileStatus(tile, rows[tile.filename], queuedIds, downloadingIds) === 'outdated'),
    [loadedTiles, rows, queuedIds, downloadingIds]
  );

  const installed = useMemo(() => {
    let count = 0;
    let bytes = 0;
    for (const row of Object.values(rows)) {
      const meta = rowInstalledTileMeta(row);
      if (!meta) continue;
      count += 1;
      bytes += meta.size_bytes;
    }
    return { count, bytes };
  }, [rows]);

  const toggleTiles = (tiles: RmdfManifestTile[]) => {
    if (tiles.length === 0) return;
    const allSelected = tiles.every((tile) => rows[tile.filename]?.selected === true);
    setRmdfTilesSelected(localStore, tiles, !allSelected);
  };

  const handleMapPress: NonNullable<MapProps['onPress']> = async (event) => {
    if (!release) return;
    const [longitude, latitude] = event.nativeEvent.lngLat;
    const chunk = chunkAt(release, longitude, latitude);
    if (!chunk || (level === 'chunk' && !boundsContain(chunkDrawBounds(chunk), longitude, latitude))) return;
    try {
      const tiles = await ensureChunks(release, [chunk]);
      if (level === 'chunk') toggleTiles(tiles);
      else if (level === 'cell') toggleTiles(tilesInCell(release, tiles, cellIdAt(longitude, latitude)));
      else {
        const filename = tileIdAt(release, longitude, latitude);
        toggleTiles(tiles.filter((tile) => tile.filename === filename));
      }
    } catch (error) {
      setChunkError(error instanceof Error ? error.message : 'Failed to load map data.');
    }
  };

  const handleRegionDidChange: NonNullable<MapProps['onRegionDidChange']> = (event) => {
    const { zoom, bounds } = event.nativeEvent;
    const [west, south, east, north] = bounds;
    setView({ zoom, bounds: { lon_min: west, lat_min: south, lon_max: east, lat_max: north } });
  };

  const confirmCellular = async (start: () => Promise<void>) => {
    try {
      if (await isCellularConnection()) {
        Alert.alert('Mobile data warning', CELLULAR_WARNING, [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Continue', style: 'default', onPress: () => void start() },
        ]);
        return;
      }
    } catch {
      // If network type cannot be determined, allow the explicit user action.
    }
    await start();
  };

  const handleDownload = async () => {
    if (loadState.kind !== 'ready' || loadState.deleteOnly || selection.downloadable.length === 0) return;
    const tiles = selection.downloadable;
    await confirmCellular(async () => {
      await rmdfDownloadManager.enqueue(tiles);
      clearRmdfTileSelection(localStore, tiles.map((tile) => tile.filename));
    });
  };

  const handleUpdate = async () => {
    if (loadState.kind !== 'ready' || loadState.deleteOnly || outdatedTiles.length === 0) return;
    await confirmCellular(() => rmdfDownloadManager.enqueue(outdatedTiles));
  };

  const handleDelete = () => {
    if (selection.deletable.length === 0) return;
    const queuedCount = selection.deletable.length - selection.deletableInstalledCount;
    const detail = queuedCount > 0 ? ` This also removes ${queuedCount} queued tile(s).` : '';
    Alert.alert('Delete map data', `Delete ${selection.deletableInstalledCount} downloaded map tile(s)?${detail}`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Delete',
        style: 'destructive',
        onPress: () => void rmdfDownloadManager.removeQueuedOrDelete(selection.deletable.map((tile) => tile.filename)),
      },
    ]);
  };

  const outdatedBytes = outdatedTiles.reduce((sum, tile) => sum + tile.size_bytes, 0);
  const warning = loadState.kind === 'ready' ? loadState.warning : null;

  return (
    <View className="flex-1 bg-background">
      <Map
        attribution={false}
        compass={false}
        logo={false}
        mapStyle={ridiMapStyle}
        onPress={handleMapPress}
        onRegionDidChange={handleRegionDidChange}
        style={StyleSheet.absoluteFill}
      >
        <Images
          images={{
            'military-hatch': require('@/assets/map/pattern-military-hatch.png'),
            'park-dot': require('@/assets/map/pattern-park-dot.png'),
          }}
        />
        {cameraCenter ? <Camera initialViewState={{ center: cameraCenter, zoom: INITIAL_ZOOM }} /> : null}
        {release ? (
          <>
            <GeoJSONSource id="rmdf-tile-states" data={tileGeoJson as never}>
              <Layer id="rmdf-tile-state-fills" type="fill" style={tileFillStyle as never} />
            </GeoJSONSource>
            <GeoJSONSource id="rmdf-grid" data={gridGeoJson as never}>
              <Layer id="rmdf-grid-lines" type="line" filter={SOLID_GRID_FILTER as never} style={gridLineStyle as never} />
              <Layer id="rmdf-grid-partly-selected" type="line" filter={PARTLY_SELECTED_FILTER as never} style={partlySelectedLineStyle as never} />
            </GeoJSONSource>
            <GeoJSONSource id="rmdf-grid-labels" data={labelGeoJson as never}>
              <Layer id="rmdf-grid-label-text" type="symbol" style={labelStyle as never} />
            </GeoJSONSource>
          </>
        ) : null}
      </Map>

      <View className="absolute left-4 right-4 rounded-3xl border border-border bg-card/95 p-4 shadow-lg" style={{ top: insets.top + 12 }}>
        <View className="flex-row items-center justify-between gap-3">
          <Pressable className="rounded-2xl bg-muted px-4 py-2" onPress={() => router.back()}>
            <Text className="text-foreground font-semibold">Back</Text>
          </Pressable>
          <View className="flex-1">
            <Text className="text-foreground text-base font-bold">Routing data download</Text>
            <Text className="text-muted text-xs">
              {release ? `${release.tile_count} tiles · ${formatBytes(release.total_bytes)} · release ${release.release_id}` : 'RMDF map tiles'}
            </Text>
          </View>
          {loadState.kind === 'loading' ? <ActivityIndicator /> : null}
        </View>
        {release ? (
          <>
            <Text className="text-muted mt-2 text-xs">{LEVEL_HINT[level]}</Text>
            <View className="mt-2 flex-row flex-wrap gap-x-3 gap-y-1">
              <LegendItem color={TILE_COLORS.downloaded} label="Downloaded" />
              <LegendItem color={TILE_COLORS.selected} label="Selected" />
              <LegendItem color={TILE_COLORS.outdated} label="Update available" />
              <LegendItem color={TILE_COLORS.pending} label="Downloading" />
            </View>
          </>
        ) : null}
        {warning ? <Text className="mt-3 text-xs font-semibold text-orange-500">{warning}</Text> : null}
        {chunkError ? <Text className="mt-3 text-xs font-semibold text-orange-500">{chunkError}</Text> : null}
        {downloads.warning ? <Text className="mt-3 text-xs font-semibold text-orange-500">{downloads.warning}</Text> : null}
      </View>

      <View className="absolute bottom-0 left-0 right-0 rounded-t-3xl border border-border bg-card/95 p-4 shadow-lg" style={{ paddingBottom: insets.bottom + 16 }}>
        {loadState.kind === 'error' ? (
          <View className="gap-3">
            <Text className="text-foreground font-bold">Could not load map data</Text>
            <Text className="text-muted text-sm">{loadState.message}</Text>
            <Pressable className="items-center rounded-2xl bg-primary px-4 py-3" onPress={() => void load()}>
              <Text className="font-bold text-white">Retry</Text>
            </Pressable>
          </View>
        ) : (
          <ScrollView className="max-h-80" contentContainerClassName="gap-3">
            <View className="flex-row flex-wrap justify-between gap-3">
              <Stat label="Selected" value={String(selection.tiles.length)} />
              <Stat label="Downloaded" value={String(installed.count)} />
              <Stat label="Stored" value={formatBytes(installed.bytes)} />
            </View>
            {downloads.active ? (
              <Text className="text-foreground text-sm font-semibold">
                Downloading map data: {downloads.remainingCount} tiles remaining · {formatBytes(downloads.remainingBytes)}
              </Text>
            ) : null}
            {downloads.lastSummary ? <Text className="text-primary text-sm font-semibold">{downloads.lastSummary}</Text> : null}
            {outdatedTiles.length > 0 ? (
              <ActionButton
                disabled={loadState.kind !== 'ready' || loadState.deleteOnly}
                label={`Update ${outdatedTiles.length} outdated tile(s) · ${formatBytes(outdatedBytes)}`}
                onPress={() => void handleUpdate()}
                variant="warning"
              />
            ) : null}
            <View className="flex-row gap-3">
              <ActionButton
                disabled={loadState.kind !== 'ready' || loadState.deleteOnly || selection.downloadable.length === 0}
                label={selection.downloadable.length > 0 ? `Download ${formatBytes(selection.downloadableBytes)}` : 'Download'}
                onPress={() => void handleDownload()}
                variant="primary"
              />
              <ActionButton
                disabled={selection.deletable.length === 0}
                label={selection.deletable.length > 0 ? `Delete ${selection.deletable.length}` : 'Delete'}
                onPress={handleDelete}
                variant="danger"
              />
              <ActionButton
                disabled={selection.tiles.length === 0}
                label="Clear"
                onPress={() => clearRmdfTileSelection(localStore, selection.tiles.map((tile) => tile.filename))}
                variant="neutral"
              />
            </View>
            {loadState.kind === 'ready' && loadState.deleteOnly ? (
              <Text className="text-muted text-xs">Downloads are disabled while using cached map data index.</Text>
            ) : null}
          </ScrollView>
        )}
      </View>
    </View>
  );
}

function LegendItem({ color, label }: { color: string; label: string }) {
  return (
    <View className="flex-row items-center gap-1">
      <View className="h-3 w-3 rounded-sm" style={{ backgroundColor: color, opacity: 0.7 }} />
      <Text className="text-muted text-xs">{label}</Text>
    </View>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <View className="min-w-24 rounded-2xl bg-muted px-3 py-2">
      <Text className="text-muted-foreground text-xs">{label}</Text>
      <Text className="text-foreground text-sm font-bold">{value}</Text>
    </View>
  );
}

const ACTION_VARIANTS = {
  primary: 'bg-primary',
  danger: 'bg-red-600',
  warning: 'bg-amber-500',
  neutral: 'bg-gray-500',
} as const;

function ActionButton({ disabled, label, onPress, variant }: { disabled: boolean; label: string; onPress: () => void; variant: keyof typeof ACTION_VARIANTS }) {
  const className = disabled
    ? 'flex-1 items-center rounded-2xl bg-muted px-4 py-3 opacity-50'
    : `flex-1 items-center rounded-2xl px-4 py-3 ${ACTION_VARIANTS[variant]}`;
  return (
    <Pressable className={className} disabled={disabled} onPress={onPress}>
      <Text className={disabled ? 'text-muted-foreground font-bold' : 'font-bold text-white'}>{label}</Text>
    </Pressable>
  );
}

const LOCATION_TIMEOUT_MS = 3000;

async function resolveInitialCamera(release: RmdfRelease): Promise<[number, number]> {
  const timeout = new Promise<[number, number]>((resolve) => setTimeout(() => resolve(releaseCenter(release)), LOCATION_TIMEOUT_MS));
  return Promise.race([currentLocationOr(release), timeout]);
}

async function currentLocationOr(release: RmdfRelease): Promise<[number, number]> {
  try {
    const permission = await Location.requestForegroundPermissionsAsync();
    if (permission.granted) {
      const lastKnown = await Location.getLastKnownPositionAsync({ maxAge: 10 * 60 * 1000 });
      if (lastKnown) return [lastKnown.coords.longitude, lastKnown.coords.latitude];
      const current = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
      return [current.coords.longitude, current.coords.latitude];
    }
  } catch {
    // Fall back to the release center.
  }
  return releaseCenter(release);
}
