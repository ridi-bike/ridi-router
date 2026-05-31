import { Camera, GeoJSONSource, Images, Layer, Map, type MapProps } from '@maplibre/maplibre-react-native';
import * as Location from 'expo-location';
import { router } from 'expo-router';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { ridiMapStyle } from '@/map/ridiMapStyle';
import { CELLULAR_WARNING } from '@/rmdf/constants';
import { isCellularConnection, rmdfDownloadManager } from '@/rmdf/downloads';
import { loadRmdfManifest } from '@/rmdf/manifest';
import { getDownloadedTileIds } from '@/rmdf/storage';
import { buildTileViewModels, formatBytes, getManifestCenter, hitTestTile, splitSelectedTiles, tilesToGeoJson } from '@/rmdf/tiles';
import type { RmdfManifest, RmdfManifestTile } from '@/rmdf/types';
import { useRmdfDownloads } from '@/rmdf/RmdfDownloadProvider';

type LoadState =
  | { kind: 'loading' }
  | { kind: 'ready'; manifest: RmdfManifest; deleteOnly: boolean; warning?: string }
  | { kind: 'error'; message: string };

const fillStyle = {
  fillColor: [
    'case',
    ['==', ['get', 'status'], 'failed'],
    'rgba(239, 68, 68, 0.45)',
    ['all', ['==', ['get', 'selected'], true], ['==', ['get', 'status'], 'downloaded']],
    'rgba(249, 115, 22, 0.45)',
    ['==', ['get', 'selected'], true],
    'rgba(32, 138, 239, 0.42)',
    ['==', ['get', 'status'], 'downloading'],
    'rgba(32, 138, 239, 0.55)',
    ['==', ['get', 'status'], 'queued'],
    'rgba(59, 130, 246, 0.32)',
    ['==', ['get', 'status'], 'downloaded'],
    'rgba(34, 197, 94, 0.32)',
    'rgba(0, 0, 0, 0)',
  ],
  fillOutlineColor: 'rgba(255,255,255,0)',
} as const;

const lineStyle = {
  lineColor: [
    'case',
    ['==', ['get', 'status'], 'failed'],
    '#ef4444',
    ['all', ['==', ['get', 'selected'], true], ['==', ['get', 'status'], 'downloaded']],
    '#f97316',
    ['==', ['get', 'selected'], true],
    '#208AEF',
    ['==', ['get', 'status'], 'downloaded'],
    '#22c55e',
    ['==', ['get', 'status'], 'queued'],
    '#3b82f6',
    '#6b7280',
  ],
  lineWidth: ['case', ['==', ['get', 'selected'], true], 2.5, 1],
} as const;

const labelStyle = {
  textField: ['get', 'label'],
  textSize: 12,
  textColor: '#ffffff',
  textHaloColor: '#1d4ed8',
  textHaloWidth: 1.5,
} as const;

export default function RmdfDownloadScreen() {
  const insets = useSafeAreaInsets();
  const downloads = useRmdfDownloads();
  const [loadState, setLoadState] = useState<LoadState>({ kind: 'loading' });
  const [downloadedIds, setDownloadedIds] = useState<Set<string>>(new Set());
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [cameraCenter, setCameraCenter] = useState<[number, number]>([11.5761, 48.1372]);

  const load = useCallback(async () => {
    setLoadState({ kind: 'loading' });
    try {
      const result = await loadRmdfManifest();
      const downloaded = await getDownloadedTileIds(result.manifest.tiles);
      setDownloadedIds(downloaded);
      setLoadState({ kind: 'ready', manifest: result.manifest, deleteOnly: result.deleteOnly, warning: result.warning });
      setCameraCenter(await resolveInitialCamera(result.manifest));
    } catch (error) {
      setLoadState({ kind: 'error', message: error instanceof Error ? error.message : 'Failed to load manifest.' });
    }
  }, []);

  useEffect(() => {
    const timeout = setTimeout(() => {
      void load();
    }, 0);
    return () => clearTimeout(timeout);
  }, [load]);

  const manifest = loadState.kind === 'ready' ? loadState.manifest : null;
  const effectiveDownloadedIds = useMemo(
    () => new Set([...downloadedIds, ...downloads.completedIds]),
    [downloadedIds, downloads.completedIds]
  );
  const split = useMemo(
    () => splitSelectedTiles(manifest?.tiles ?? [], selectedIds, effectiveDownloadedIds),
    [manifest?.tiles, selectedIds, effectiveDownloadedIds]
  );
  const selectedQueuedTiles = useMemo(
    () => split.selectedTiles.filter((tile) => downloads.queuedIds.has(tile.filename)),
    [split.selectedTiles, downloads.queuedIds]
  );
  const selectedDeletableTiles = useMemo(() => {
    const byFilename = new globalThis.Map<string, RmdfManifestTile>();
    for (const tile of split.selectedDownloadedTiles) byFilename.set(tile.filename, tile);
    for (const tile of selectedQueuedTiles) byFilename.set(tile.filename, tile);
    return [...byFilename.values()];
  }, [split.selectedDownloadedTiles, selectedQueuedTiles]);
  const selectedDownloadableTiles = useMemo(
    () => split.selectedMissingTiles.filter((tile) => !downloads.queuedIds.has(tile.filename) && !downloads.downloadingIds.has(tile.filename)),
    [split.selectedMissingTiles, downloads.queuedIds, downloads.downloadingIds]
  );
  const selectedDownloadableBytes = useMemo(
    () => selectedDownloadableTiles.reduce((sum, tile) => sum + tile.size_bytes, 0),
    [selectedDownloadableTiles]
  );

  const tileViewModels = useMemo(() => {
    if (!manifest) return [];
    return buildTileViewModels({
      tiles: manifest.tiles,
      downloadedIds: effectiveDownloadedIds,
      selectedIds,
      queuedIds: downloads.queuedIds,
      downloadingIds: downloads.downloadingIds,
      failedIds: downloads.failedIds,
      queuePositions: downloads.queuePositions,
    });
  }, [manifest, effectiveDownloadedIds, selectedIds, downloads.queuedIds, downloads.downloadingIds, downloads.failedIds, downloads.queuePositions]);

  const geoJson = useMemo(() => tilesToGeoJson(tileViewModels), [tileViewModels]);

  const handleMapPress: NonNullable<MapProps['onPress']> = (event) => {
    if (!manifest) return;
    const [longitude, latitude] = event.nativeEvent.lngLat;
    const tile = hitTestTile(manifest.tiles, longitude, latitude);
    if (!tile) return;
    setSelectedIds((previous) => {
      const next = new Set(previous);
      if (next.has(tile.filename)) next.delete(tile.filename);
      else next.add(tile.filename);
      return next;
    });
  };

  const handleDownload = async () => {
    if (loadState.kind !== 'ready' || loadState.deleteOnly || selectedDownloadableTiles.length === 0) return;
    const start = async () => {
      await rmdfDownloadManager.enqueue(selectedDownloadableTiles);
      setSelectedIds(new Set());
    };
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

  const handleDelete = () => {
    if (selectedDeletableTiles.length === 0) return;
    const downloadedCount = split.selectedDownloadedTiles.length;
    const queuedCount = selectedQueuedTiles.length;
    const detail = queuedCount > 0 ? ` This also removes ${queuedCount} queued tile(s).` : '';
    Alert.alert('Delete map data', `Delete ${downloadedCount} downloaded map tile(s)?${detail}`, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Delete',
        style: 'destructive',
        onPress: () => {
          void deleteSelectedTiles(selectedDeletableTiles);
        },
      },
    ]);
  };

  const deleteSelectedTiles = async (tiles: RmdfManifestTile[]) => {
    const filenames = tiles.map((tile) => tile.filename);
    const result = await rmdfDownloadManager.removeQueuedOrDelete(filenames);
    setDownloadedIds((previous) => {
      const next = new Set(previous);
      for (const filename of result.deleted) next.delete(filename);
      return next;
    });
    setSelectedIds((previous) => {
      const next = new Set(previous);
      for (const filename of filenames) next.delete(filename);
      return next;
    });
  };

  const warning = loadState.kind === 'ready' ? loadState.warning : null;
  const downloadedSize = useMemo(() => {
    if (!manifest) return 0;
    return manifest.tiles.filter((tile) => effectiveDownloadedIds.has(tile.filename)).reduce((sum, tile) => sum + tile.size_bytes, 0);
  }, [manifest, effectiveDownloadedIds]);

  return (
    <View className="flex-1 bg-background">
      <Map attribution={false} compass={false} logo={false} mapStyle={ridiMapStyle} onPress={handleMapPress} style={StyleSheet.absoluteFill}>
        <Images
          images={{
            'military-hatch': require('@/assets/map/pattern-military-hatch.png'),
            'park-dot': require('@/assets/map/pattern-park-dot.png'),
          }}
        />
        <Camera initialViewState={{ center: cameraCenter, zoom: 9 }} />
        {manifest ? (
          <GeoJSONSource id="rmdf-tiles" data={geoJson as never}>
            <Layer id="rmdf-tile-fills" type="fill" style={fillStyle as never} />
            <Layer id="rmdf-tile-borders" type="line" style={lineStyle as never} />
            <Layer id="rmdf-tile-labels" type="symbol" minzoom={8} style={labelStyle as never} />
          </GeoJSONSource>
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
              {loadState.kind === 'ready' ? `${loadState.manifest.tiles.length} tiles available` : 'RMDF map tiles'}
            </Text>
          </View>
          {loadState.kind === 'loading' ? <ActivityIndicator /> : null}
        </View>
        {warning ? <Text className="mt-3 text-xs font-semibold text-orange-500">{warning}</Text> : null}
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
          <ScrollView className="max-h-72" contentContainerClassName="gap-3">
            <View className="flex-row flex-wrap justify-between gap-3">
              <Stat label="Selected" value={String(split.selectedTiles.length)} />
              <Stat label="Downloaded" value={`${effectiveDownloadedIds.size}/${manifest?.tiles.length ?? 0}`} />
              <Stat label="Stored" value={formatBytes(downloadedSize)} />
            </View>
            {downloads.active ? (
              <Text className="text-foreground text-sm font-semibold">
                Downloading map data: {downloads.completedCount}/{downloads.totalCount} tiles · {formatBytes(downloads.completedBytes)} / {formatBytes(downloads.totalBytes)}
              </Text>
            ) : null}
            {downloads.lastSummary ? <Text className="text-primary text-sm font-semibold">{downloads.lastSummary}</Text> : null}
            <View className="flex-row gap-3">
              <ActionButton
                disabled={loadState.kind !== 'ready' || loadState.deleteOnly || selectedDownloadableTiles.length === 0}
                label={selectedDownloadableTiles.length > 0 ? `Download ${formatBytes(selectedDownloadableBytes)}` : 'Download'}
                onPress={() => void handleDownload()}
                variant="primary"
              />
              <ActionButton
                disabled={selectedDeletableTiles.length === 0}
                label="Delete"
                onPress={handleDelete}
                variant="danger"
              />
            </View>
            {loadState.kind === 'ready' && loadState.deleteOnly ? (
              <Text className="text-muted text-xs">Downloads are disabled while using cached manifest data.</Text>
            ) : null}
          </ScrollView>
        )}
      </View>
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

function ActionButton({ disabled, label, onPress, variant }: { disabled: boolean; label: string; onPress: () => void; variant: 'primary' | 'danger' }) {
  const className = disabled
    ? 'flex-1 items-center rounded-2xl bg-muted px-4 py-3 opacity-50'
    : variant === 'danger'
      ? 'flex-1 items-center rounded-2xl bg-red-600 px-4 py-3'
      : 'flex-1 items-center rounded-2xl bg-primary px-4 py-3';
  return (
    <Pressable className={className} disabled={disabled} onPress={onPress}>
      <Text className={disabled ? 'text-muted-foreground font-bold' : 'font-bold text-white'}>{label}</Text>
    </Pressable>
  );
}

async function resolveInitialCamera(manifest: RmdfManifest): Promise<[number, number]> {
  try {
    const permission = await Location.requestForegroundPermissionsAsync();
    if (permission.granted) {
      const lastKnown = await Location.getLastKnownPositionAsync({ maxAge: 10 * 60 * 1000 });
      if (lastKnown) return [lastKnown.coords.longitude, lastKnown.coords.latitude];
      const current = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
      return [current.coords.longitude, current.coords.latitude];
    }
  } catch {
    // Fall back to manifest center.
  }
  return getManifestCenter(manifest);
}
