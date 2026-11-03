import { Camera, GeoJSONSource, Images, Layer, Map, Marker, type CameraRef, type MapProps, type MapRef } from '@maplibre/maplibre-react-native';
import * as FileSystem from 'expo-file-system/legacy';
import * as Location from 'expo-location';
import * as Sharing from 'expo-sharing';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Animated, FlatList, Pressable, ScrollView, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { configureMapLibreLogs } from '@/map/mapLibreLogs';
import { POI_ICON_IMAGES } from '@/map/poiIcons';
import { INTERACTIVE_MAP_LAYER_IDS, ridiMapStyle } from '@/map/ridiMapStyle';
import type { RmdfManifestTile } from '@/rmdf/types';
import { endpointBounds, formatCoordinate, formatDistance, planFinishPoint, planStartPoint, routeBounds } from '@/routes/format';
import { generatedRoutesGeoJson, planningGuideGeoJson } from '@/routes/geojson';
import { DEFAULT_ROUTE_RULE_PRESET_ID, getRouteRulePreset, ROUTE_RULE_PRESET_ORDER, ROUTE_RULE_PRESETS } from '@/routes/rulePresets';
import { routeRowToNormalized } from '@/routes/persistence';
import { MISSING_FINISH_TILE_WARNING, MISSING_START_TILE_WARNING, MISSING_TILE_WARNING, validateDownloadedTileCoverage } from '@/routes/tileCoverage';
import type { NormalizedRoute, RouteDraft, RouteFeatureKind, RoutePoint } from '@/routes/types';
import { useRouteGeneration } from '@/routes/RouteGenerationProvider';
import { useRmdfInstalledTiles, useSyncedPlan, useSyncedRoutesForPlan } from '@/tinybase/hooks';

configureMapLibreLogs();

type SelectedFeatureKind = 'coordinate' | 'poi' | 'name';

type SelectedPoint = {
  latitude: number;
  longitude: number;
  kind: SelectedFeatureKind;
  classification: string;
  name?: string;
  details: DetailRow[];
  tileId?: string;
  hasDownloadedTile: boolean;
};

type DetailRow = { label: string; value: string };
type UserLocation = { latitude: number; longitude: number };
type MapRouteMode =
  | { kind: 'idle' }
  | { kind: 'point_selected'; selectedPoint: SelectedPoint }
  | { kind: 'planning'; draft: RouteDraft }
  | { kind: 'generating'; draft: Required<RouteDraft> }
  | { kind: 'route_review'; planId: string; selectedRouteIndex: number };

type FeatureLike = {
  geometry?: { type?: string; coordinates?: unknown } | null;
  layer?: { id?: string };
  properties?: Record<string, unknown> | null;
};

const DETAIL_FIELD_ORDER = ['name', 'name:en', 'kind', 'kind_detail', 'amenity', 'tourism', 'shop', 'cuisine', 'sport', 'religion', 'population', 'capital', 'wikidata', 'wikipedia'] as const;
const GPX_EXPORT_DIRECTORY = 'ridi-routes/gpx/';

function stringValue(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim().length > 0) return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return undefined;
}

function featureName(properties: Record<string, unknown>): string | undefined {
  return stringValue(properties['name:en']) ?? stringValue(properties.name);
}

function detailLabel(field: string): string {
  return field.replaceAll('_', ' ').replaceAll(':', ' ');
}

function allDetailRows(feature: FeatureLike, properties: Record<string, unknown>): DetailRow[] {
  const rows: DetailRow[] = [];
  const addedFields = new Set<string>();
  if (feature.layer?.id) rows.push({ label: 'map layer', value: feature.layer.id });
  for (const field of DETAIL_FIELD_ORDER) {
    const value = stringValue(properties[field]);
    if (value) {
      rows.push({ label: detailLabel(field), value });
      addedFields.add(field);
    }
  }
  for (const [field, rawValue] of Object.entries(properties)) {
    if (addedFields.has(field) || field.startsWith('name:')) continue;
    const value = stringValue(rawValue);
    if (value) rows.push({ label: detailLabel(field), value });
  }
  return rows;
}

function poiClassification(properties: Record<string, unknown>): string {
  return stringValue(properties.kind_detail) ?? stringValue(properties.amenity) ?? stringValue(properties.tourism) ?? stringValue(properties.shop) ?? stringValue(properties.kind) ?? 'POI';
}

function nameClassification(feature: FeatureLike, properties: Record<string, unknown>): string {
  if (feature.layer?.id === 'country_names') return 'Country';
  if (feature.layer?.id === 'city_names' || feature.layer?.id === 'city_dots') return 'City';
  return stringValue(properties.kind_detail) ?? stringValue(properties.kind) ?? 'Place name';
}

function selectedFeatureDetails(feature: FeatureLike | undefined): Pick<SelectedPoint, 'kind' | 'classification' | 'name' | 'details'> {
  if (!feature?.properties) return { kind: 'coordinate', classification: 'Coordinate', details: [] };
  const name = featureName(feature.properties);
  if (feature.layer?.id === 'poi_names' || feature.layer?.id === 'poi_dots') {
    return { kind: 'poi', classification: `POI · ${poiClassification(feature.properties)}`, name, details: allDetailRows(feature, feature.properties) };
  }
  return { kind: 'name', classification: nameClassification(feature, feature.properties), name, details: allDetailRows(feature, feature.properties) };
}

function selectedFeatureCoordinate(feature: FeatureLike | undefined): Pick<SelectedPoint, 'latitude' | 'longitude'> | undefined {
  if (feature?.geometry?.type !== 'Point' || !Array.isArray(feature.geometry.coordinates)) return undefined;
  const [longitude, latitude] = feature.geometry.coordinates;
  return typeof latitude === 'number' && typeof longitude === 'number' ? { latitude, longitude } : undefined;
}

function drawerTitle(selectedPoint: SelectedPoint): string {
  if (selectedPoint.name) return selectedPoint.name;
  if (selectedPoint.kind === 'poi') return 'Selected POI';
  if (selectedPoint.kind === 'name') return 'Selected place';
  return 'Selected point';
}

function routePointFromSelection(point: SelectedPoint): RoutePoint {
  return {
    latitude: point.latitude,
    longitude: point.longitude,
    label: point.name ?? formatCoordinate(point.latitude, point.longitude),
    source: 'map_selection',
    featureKind: point.kind as RouteFeatureKind,
    tileId: point.tileId,
  };
}

function currentLocationPoint(location: UserLocation, tiles: RmdfManifestTile[], downloadedTileIds: Set<string>): RoutePoint | undefined {
  const coverage = validateDownloadedTileCoverage({ tiles, downloadedTileIds, latitude: location.latitude, longitude: location.longitude });
  if (!coverage.ok) return undefined;
  return { latitude: location.latitude, longitude: location.longitude, label: 'Current location', source: 'current_location', featureKind: 'coordinate', tileId: coverage.tileId };
}

function tilesCenter(tiles: RmdfManifestTile[]): [number, number] | null {
  if (tiles.length === 0) return null;
  let [west, south, east, north] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const { bounds } of tiles) {
    west = Math.min(west, bounds.lon_min);
    south = Math.min(south, bounds.lat_min);
    east = Math.max(east, bounds.lon_max);
    north = Math.max(north, bounds.lat_max);
  }
  return [(west + east) / 2, (south + north) / 2];
}

function boundsWithPadding(bounds: [number, number, number, number]): [number, number, number, number] {
  const [w, s, e, n] = bounds;
  const dx = Math.max((e - w) * 0.18, 0.003);
  const dy = Math.max((n - s) * 0.18, 0.003);
  return [w - dx, s - dy, e + dx, n + dy];
}

function topStat(stats: unknown): string | null {
  if (!stats || typeof stats !== 'object') return null;
  const entries = Object.entries(stats as Record<string, { percentage?: number; len_m?: number }>).sort((a, b) => Number(b[1]?.len_m ?? 0) - Number(a[1]?.len_m ?? 0));
  const [name, value] = entries[0] ?? [];
  if (!name) return null;
  return `${name} ${Math.round(Number(value?.percentage ?? 0))}%`;
}

function xmlEscape(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&apos;');
}

function fileSafe(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'route';
}

function routeGpx(route: NormalizedRoute, name: string): string {
  const points = route.coords.map(([latitude, longitude]) => `    <trkpt lat="${latitude}" lon="${longitude}" />`).join('\n');
  return `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="Ridi" xmlns="http://www.topografix.com/GPX/1/1">
  <metadata>
    <name>${xmlEscape(name)}</name>
  </metadata>
  <trk>
    <name>${xmlEscape(name)}</name>
    <trkseg>
${points}
    </trkseg>
  </trk>
</gpx>
`;
}

export default function RideScreen() {
  const { height: windowHeight, width: windowWidth } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const cameraRef = useRef<CameraRef>(null);
  const mapRef = useRef<MapRef>(null);
  const [bottomDrawerTranslateY] = useState(() => new Animated.Value(360));
  const [topDrawerTranslateY] = useState(() => new Animated.Value(-360));
  const [mode, setMode] = useState<MapRouteMode>({ kind: 'idle' });
  const [userLocation, setUserLocation] = useState<UserLocation | null>(null);
  const manifestTiles = useRmdfInstalledTiles();
  const downloadedTileIds = useMemo(() => new Set(manifestTiles.map((tile) => tile.filename)), [manifestTiles]);
  const routeGeneration = useRouteGeneration();
  const selectedPlan = useSyncedPlan(routeGeneration.selectedPlanId);
  const selectedRouteRows = useSyncedRoutesForPlan(routeGeneration.selectedPlanId);
  const selectedRoutes = useMemo(() => selectedRouteRows.map(routeRowToNormalized), [selectedRouteRows]);
  const sortedRoutes = useMemo(() => [...selectedRoutes].sort((a, b) => a.lengthM - b.lengthM), [selectedRoutes]);

  const planningDraft = mode.kind === 'planning' || mode.kind === 'generating' ? mode.draft : routeGeneration.draft;
  const reviewStart = selectedPlan ? planStartPoint(selectedPlan) : undefined;
  const reviewFinish = selectedPlan ? planFinishPoint(selectedPlan) : undefined;
  const showRouteOverlay = mode.kind === 'planning' || mode.kind === 'generating' || !!selectedPlan;
  const visualStart = showRouteOverlay ? (reviewStart ?? planningDraft.start) : undefined;
  const visualFinish = showRouteOverlay ? (reviewFinish ?? planningDraft.finish) : undefined;
  const generatedRoutes = useMemo(() => generatedRoutesGeoJson(selectedRoutes, routeGeneration.selectedRouteIndex, !!selectedPlan), [selectedRoutes, routeGeneration.selectedRouteIndex, selectedPlan]);
  const planningGuide = useMemo(() => planningGuideGeoJson(visualStart, visualFinish), [visualStart, visualFinish]);

  const fitBounds = useCallback((bounds: [number, number, number, number], bottomPadding = 220) => {
    cameraRef.current?.fitBounds(boundsWithPadding(bounds), { padding: { top: 130, right: 40, bottom: bottomPadding, left: 40 }, duration: 650 });
  }, []);

  useEffect(() => {
    const showBottom = mode.kind === 'point_selected';
    Animated.timing(bottomDrawerTranslateY, { duration: 220, toValue: showBottom ? 0 : 360, useNativeDriver: true }).start();
    const showTop = mode.kind === 'planning' || mode.kind === 'generating' || mode.kind === 'route_review' || !!routeGeneration.selectedPlanId;
    Animated.timing(topDrawerTranslateY, { duration: 220, toValue: showTop ? 0 : -360, useNativeDriver: true }).start();
  }, [bottomDrawerTranslateY, mode, routeGeneration.selectedPlanId, topDrawerTranslateY]);

  // Until a GPS fix arrives, start over the downloaded routing data rather than a fixed city.
  // Set once the map has been centered (on the tiles or the GPS fix); after that it stays put.
  const centeredOnLocation = useRef(false);
  const [mapLoaded, setMapLoaded] = useState(false);
  const installedCenter = useMemo(() => tilesCenter(manifestTiles), [manifestTiles]);
  useEffect(() => {
    // Once only: later tile-table changes (each finished download) must not move the map.
    if (!mapLoaded || !installedCenter || centeredOnLocation.current || routeGeneration.selectedPlanId) return;
    centeredOnLocation.current = true;
    cameraRef.current?.jumpTo({ center: installedCenter, zoom: 10 });
  }, [mapLoaded, installedCenter, routeGeneration.selectedPlanId]);

  useEffect(() => {
    let isMounted = true;
    async function requestLocationAndCenterMap() {
      const { status } = await Location.requestForegroundPermissionsAsync();
      if (status !== Location.PermissionStatus.GRANTED) return;
      const location = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
      if (!isMounted) return;
      const nextUserLocation = { latitude: location.coords.latitude, longitude: location.coords.longitude };
      setUserLocation(nextUserLocation);
      centeredOnLocation.current = true;
      cameraRef.current?.easeTo({ center: [nextUserLocation.longitude, nextUserLocation.latitude], duration: 700, zoom: 14 });
    }
    requestLocationAndCenterMap().catch(() => undefined);
    return () => { isMounted = false; };
  }, []);


  useEffect(() => {
    if (!visualStart || !visualFinish) return;
    const generatedBounds = selectedRoutes.length ? routeBounds(selectedRoutes) : null;
    fitBounds(generatedBounds ?? endpointBounds(visualStart, visualFinish));
  }, [fitBounds, selectedRoutes, visualFinish, visualStart]);

  const handleMapPress: NonNullable<MapProps['onPress']> = async (event) => {
    if (routeGeneration.isGenerating) return;
    const [longitude, latitude] = event.nativeEvent.lngLat;
    const point = event.nativeEvent.point ?? (await mapRef.current?.project([longitude, latitude]));
    if (!point) return;

    let feature: FeatureLike | undefined;
    for (const layerId of INTERACTIVE_MAP_LAYER_IDS) {
      const features = await mapRef.current?.queryRenderedFeatures(point, { layers: [layerId] });
      const layerFeature = features?.[0] as FeatureLike | undefined;
      if (layerFeature?.properties) {
        feature = { ...layerFeature, layer: { ...layerFeature.layer, id: layerFeature.layer?.id ?? layerId } };
        break;
      }
    }

    const selectedDetails = selectedFeatureDetails(feature);
    const selectedCoordinate = selectedFeatureCoordinate(feature) ?? { latitude, longitude };
    const coverage = validateDownloadedTileCoverage({ tiles: manifestTiles, downloadedTileIds, latitude: selectedCoordinate.latitude, longitude: selectedCoordinate.longitude });
    if (!coverage.ok) routeGeneration.showWarning(MISSING_TILE_WARNING);
    setMode({ kind: 'point_selected', selectedPoint: { ...selectedCoordinate, ...selectedDetails, tileId: coverage.ok ? coverage.tileId : undefined, hasDownloadedTile: coverage.ok } });
  };

  const beginRideTo = (selectedPoint: SelectedPoint) => {
    if (!selectedPoint.hasDownloadedTile) {
      routeGeneration.showWarning(MISSING_TILE_WARNING);
      return;
    }
    const finish = routePointFromSelection(selectedPoint);
    const start = routeGeneration.draft.start ?? (userLocation ? currentLocationPoint(userLocation, manifestTiles, downloadedTileIds) : undefined);
    const nextDraft: RouteDraft = { ...routeGeneration.draft, start, finish, rulePresetId: routeGeneration.draft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID };
    routeGeneration.setDraft(nextDraft);
    if (!start) routeGeneration.showWarning(MISSING_START_TILE_WARNING);
    setMode({ kind: 'planning', draft: nextDraft });
  };

  const beginRideFrom = (selectedPoint: SelectedPoint) => {
    if (!selectedPoint.hasDownloadedTile) {
      routeGeneration.showWarning(MISSING_TILE_WARNING);
      return;
    }
    const start = routePointFromSelection(selectedPoint);
    const nextDraft: RouteDraft = { ...routeGeneration.draft, start, rulePresetId: routeGeneration.draft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID };
    routeGeneration.setDraft(nextDraft);
    setMode({ kind: 'planning', draft: nextDraft });
  };

  const setDraftAndMode = (draft: RouteDraft) => {
    routeGeneration.setDraft(draft);
    setMode({ kind: 'planning', draft });
  };

  const handleGo = async () => {
    const draft = planningDraft;
    if (!draft.start || !draft.finish || routeGeneration.isGenerating) return;
    const startCoverage = validateDownloadedTileCoverage({ tiles: manifestTiles, downloadedTileIds, latitude: draft.start.latitude, longitude: draft.start.longitude });
    if (!startCoverage.ok) { routeGeneration.showWarning(MISSING_START_TILE_WARNING); return; }
    const finishCoverage = validateDownloadedTileCoverage({ tiles: manifestTiles, downloadedTileIds, latitude: draft.finish.latitude, longitude: draft.finish.longitude });
    if (!finishCoverage.ok) { routeGeneration.showWarning(MISSING_FINISH_TILE_WARNING); return; }
    const completeDraft: Required<RouteDraft> = { start: { ...draft.start, tileId: startCoverage.tileId }, finish: { ...draft.finish, tileId: finishCoverage.tileId }, rulePresetId: draft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID };
    setMode({ kind: 'generating', draft: completeDraft });
    const plan = await routeGeneration.generate(completeDraft);
    if (plan) setMode({ kind: 'route_review', planId: plan.plan_id, selectedRouteIndex: 0 });
  };

  const closeReview = () => {
    if (routeGeneration.isGenerating) return;
    routeGeneration.closeSelectedPlan();
    routeGeneration.setDraft({ rulePresetId: DEFAULT_ROUTE_RULE_PRESET_ID });
    setMode({ kind: 'idle' });
  };

  const downloadGpx = async (route: NormalizedRoute) => {
    if (!selectedPlan || !FileSystem.documentDirectory) return;
    const routeName = `${selectedPlan.start_label} to ${selectedPlan.finish_label} - route ${route.routeIndex + 1}`;
    const directory = `${FileSystem.documentDirectory}${GPX_EXPORT_DIRECTORY}`;
    const fileUri = `${directory}${fileSafe(routeName)}.gpx`;
    try {
      const directoryInfo = await FileSystem.getInfoAsync(directory);
      if (!directoryInfo.exists) await FileSystem.makeDirectoryAsync(directory, { intermediates: true });
      await FileSystem.writeAsStringAsync(fileUri, routeGpx(route, routeName));
      if (await Sharing.isAvailableAsync()) {
        await Sharing.shareAsync(fileUri, { dialogTitle: 'Download GPX', mimeType: 'application/gpx+xml' });
      } else {
        routeGeneration.showWarning(`GPX saved to ${fileUri}`);
      }
    } catch {
      routeGeneration.showWarning('Could not create GPX file.');
    }
  };


  const goEnabled = !!planningDraft.start && !!planningDraft.finish && !routeGeneration.isGenerating;
  const selectedPoint = mode.kind === 'point_selected' ? mode.selectedPoint : null;

  return (
    <View className="flex-1 bg-background">
      <Map ref={mapRef} attribution={false} compass={false} logo={false} mapStyle={ridiMapStyle} onDidFinishLoadingMap={() => setMapLoaded(true)} onPress={handleMapPress} style={StyleSheet.absoluteFill}>
        <Images images={{ 'military-hatch': require('@/assets/map/pattern-military-hatch.png'), 'park-dot': require('@/assets/map/pattern-park-dot.png'), ...POI_ICON_IMAGES }} />
        <Camera ref={cameraRef} initialViewState={{ center: [11.5761, 48.1372], zoom: 10 }} />

        <GeoJSONSource id="planning-guide" data={planningGuide as unknown as GeoJSON.GeoJSON}>
          <Layer id="planning-guide-line" type="line" style={{ lineColor: '#22c55e', lineWidth: 3, lineDasharray: [2, 2], lineOpacity: 0.8 }} />
        </GeoJSONSource>
        <GeoJSONSource id="generated-routes" data={generatedRoutes as unknown as GeoJSON.GeoJSON}>
          <Layer id="generated-routes-muted" type="line" filter={['!=', ['get', 'selected'], true]} style={{ lineColor: '#64748b', lineWidth: 4, lineOpacity: 0.45 }} />
          <Layer id="generated-routes-selected" type="line" filter={['==', ['get', 'selected'], true]} style={{ lineColor: '#16a34a', lineWidth: 7, lineOpacity: 0.95 }} />
        </GeoJSONSource>

        {userLocation ? <Marker anchor="center" id="user-location" lngLat={[userLocation.longitude, userLocation.latitude]}><View className="h-6 w-6 items-center justify-center rounded-full bg-blue-500/25"><View className="h-3.5 w-3.5 rounded-full border-2 border-white bg-blue-500" /></View></Marker> : null}
        {visualStart ? <Marker anchor="bottom" id="route-start" lngLat={[visualStart.longitude, visualStart.latitude]}><View className="items-center"><View className="h-10 w-10 items-center justify-center rounded-full border-2 border-white bg-green-500 shadow-lg"><Text className="text-lg font-black text-white">▶</Text></View><View style={[styles.pinTip, { borderTopColor: '#22c55e' }]} /></View></Marker> : null}
        {visualFinish ? <Marker anchor="bottom" id="route-finish" lngLat={[visualFinish.longitude, visualFinish.latitude]}><View className="items-center"><View className="h-10 w-10 items-center justify-center rounded-full border-2 border-white bg-green-600 shadow-lg"><Text className="text-lg font-black text-white">⚑</Text></View><View style={[styles.pinTip, { borderTopColor: '#16a34a' }]} /></View></Marker> : null}
        {selectedPoint && !visualFinish ? <Marker anchor="bottom" id="selected-point" lngLat={[selectedPoint.longitude, selectedPoint.latitude]}><View className="items-center"><View className="h-10 w-10 items-center justify-center rounded-full border-2 border-white bg-red-500 shadow-lg"><View className="h-3 w-3 rounded-full bg-neutral-900" /></View><View style={styles.pinTip} /></View></Marker> : null}
      </Map>

      <Animated.View pointerEvents={mode.kind === 'planning' || mode.kind === 'generating' || selectedPlan ? 'auto' : 'none'} className="absolute left-3 right-3 top-0 rounded-b-[28px] border border-border bg-card px-5 pb-5 pt-14 shadow-2xl" style={{ transform: [{ translateY: topDrawerTranslateY }] }}>
        {selectedPlan ? (
          <View>
            <View className="flex-row items-start justify-between gap-3">
              <View className="flex-1"><Text className="text-2xl font-semibold text-foreground">Route review</Text><Text className="mt-2 text-muted">{selectedPlan.start_label} → {selectedPlan.finish_label}</Text></View>
              {!routeGeneration.isGenerating ? <Pressable className="h-10 w-10 items-center justify-center rounded-full bg-background" onPress={closeReview}><Text className="text-3xl leading-8 text-foreground">×</Text></Pressable> : null}
            </View>
            <Text className="mt-3 text-sm text-muted">Rule: {selectedPlan.rule_label}</Text>
            <Text className="mt-1 text-sm text-muted">{selectedPlan.route_count === 0 ? '0 routes generated' : `${selectedPlan.route_count} routes · ${formatDistance(selectedPlan.total_distance_m)}`}</Text>
            {selectedPlan.generation_status === 'failed' ? <View className="mt-3 rounded-2xl bg-red-500/10 p-3"><Text className="font-semibold text-red-500">Generation failed</Text><Text className="mt-1 text-sm text-red-500">{selectedPlan.error_message}</Text>{selectedPlan.error_details ? <Text className="mt-1 text-xs text-red-400">{selectedPlan.error_details}</Text> : null}</View> : null}
          </View>
        ) : (
          <View>
            <Text className="text-2xl font-semibold text-foreground">Plan route</Text>
            {(['start', 'finish'] as const).map((field) => {
              const point = planningDraft[field];
              return <View key={field} className="mt-3 flex-row items-center rounded-2xl bg-background px-4 py-3"><View className="flex-1"><Text className="text-xs uppercase tracking-wider text-muted">{field}</Text><Text className="mt-1 text-base font-semibold text-foreground" numberOfLines={1}>{point ? point.label : field === 'start' ? 'Choose start' : 'Choose finish'}</Text></View>{point ? <Pressable className="h-9 w-9 items-center justify-center rounded-full bg-card" onPress={() => setDraftAndMode({ ...planningDraft, [field]: undefined })}><Text className="text-2xl text-foreground">×</Text></Pressable> : null}</View>;
            })}
            <View className="mt-4 flex-row gap-2">
              {ROUTE_RULE_PRESET_ORDER.map((id) => <Pressable key={id} className={(planningDraft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID) === id ? 'flex-1 rounded-full bg-primary px-2 py-3' : 'flex-1 rounded-full bg-background px-2 py-3'} onPress={() => setDraftAndMode({ ...planningDraft, rulePresetId: id })}><Text className={(planningDraft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID) === id ? 'text-center text-xs font-semibold text-primary-foreground' : 'text-center text-xs font-semibold text-foreground'}>{ROUTE_RULE_PRESETS[id].label}</Text></Pressable>)}
            </View>
            <Pressable className={goEnabled ? 'mt-4 rounded-full bg-primary py-4' : 'mt-4 rounded-full bg-muted/30 py-4'} disabled={!goEnabled} onPress={handleGo}><Text className={goEnabled ? 'text-center text-base font-semibold text-primary-foreground' : 'text-center text-base font-semibold text-muted'}>{routeGeneration.isGenerating ? 'Generating…' : `Go · ${getRouteRulePreset(planningDraft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID).label}`}</Text></Pressable>
          </View>
        )}
      </Animated.View>

      <Animated.View pointerEvents={selectedPoint ? 'auto' : 'none'} className="absolute bottom-0 left-0 right-0 rounded-t-[28px] border border-border bg-card px-5 pt-5 shadow-2xl" style={[styles.drawer, { paddingBottom: insets.bottom + 96, maxHeight: windowHeight * 0.52, transform: [{ translateY: bottomDrawerTranslateY }] }]}>
        {selectedPoint ? <><View className="flex-row items-start justify-between gap-4"><View className="flex-1"><Text className="text-3xl font-semibold text-foreground">{drawerTitle(selectedPoint)}</Text><Text className="mt-3 text-base text-muted">{selectedPoint.latitude.toFixed(6)}, {selectedPoint.longitude.toFixed(6)}</Text><Text className="mt-2 text-sm uppercase tracking-wider text-muted">{selectedPoint.classification}</Text></View><Pressable className="h-12 w-12 items-center justify-center rounded-full bg-background" onPress={() => setMode({ kind: 'idle' })}><Text className="text-4xl leading-10 text-foreground">×</Text></Pressable></View>
          <View className="mt-4 flex-row gap-3"><Pressable className="flex-1 rounded-full bg-primary py-3" onPress={() => beginRideTo(selectedPoint)}><Text className="text-center font-semibold text-primary-foreground">Ride to</Text></Pressable><Pressable className="flex-1 rounded-full bg-background py-3" onPress={() => beginRideFrom(selectedPoint)}><Text className="text-center font-semibold text-foreground">Ride from</Text></Pressable></View>
          {!selectedPoint.hasDownloadedTile ? <Text className="mt-4 rounded-2xl bg-yellow-100 p-3 text-sm font-semibold text-yellow-950">No downloaded routing tile exists for this point.</Text> : null}
          {selectedPoint.details.length > 0 ? <ScrollView className="mt-5" contentContainerStyle={styles.detailsContent} showsVerticalScrollIndicator><View className="gap-2">{selectedPoint.details.map((detail) => <View className="flex-row justify-between gap-4" key={detail.label}><Text className="text-base capitalize text-muted">{detail.label}</Text><Text className="flex-1 text-right text-base text-foreground">{detail.value}</Text></View>)}</View></ScrollView> : null}</> : null}
      </Animated.View>

      {selectedPlan ? <View className="absolute bottom-24 left-0 right-0"><FlatList horizontal pagingEnabled showsHorizontalScrollIndicator={false} data={sortedRoutes.length ? sortedRoutes : [{ routeIndex: 0 } as NormalizedRoute]} keyExtractor={(item) => String(item.routeIndex)} onMomentumScrollEnd={(event) => { const index = Math.round(event.nativeEvent.contentOffset.x / event.nativeEvent.layoutMeasurement.width); if (sortedRoutes[index]) routeGeneration.setSelectedRouteIndex(sortedRoutes[index].routeIndex); }} renderItem={({ item }) => <View style={{ width: windowWidth }} className="px-4"><View className="rounded-3xl border border-border bg-card p-5 shadow-xl">{selectedPlan.generation_status === 'failed' ? <><Text className="text-lg font-semibold text-red-500">Route generation failed</Text><Text className="mt-2 text-muted">{selectedPlan.error_message}</Text>{selectedPlan.error_details ? <Text className="mt-2 text-xs text-muted">{selectedPlan.error_details}</Text> : null}</> : sortedRoutes.length === 0 ? <><Text className="text-lg font-semibold text-foreground">0 routes generated</Text><Text className="mt-2 text-muted">Try different endpoints or rules.</Text></> : <><Text className="text-lg font-semibold text-foreground">Route {item.routeIndex + 1}</Text><Text className="mt-2 text-muted">{formatDistance(item.lengthM)} · score {item.score.toFixed(1)} · {item.junctionCount} junctions</Text><Text className="mt-1 text-muted">{[topStat(item.highwayStats), topStat(item.surfaceStats), topStat(item.smoothnessStats)].filter(Boolean).join(' · ')}</Text>{item.cluster !== undefined ? <Text className="mt-1 text-muted">Cluster {item.cluster}</Text> : null}<Pressable className="mt-4 rounded-full bg-primary py-3" onPress={() => downloadGpx(item)}><Text className="text-center font-semibold text-primary-foreground">Download GPX</Text></Pressable></>}</View></View>} /></View> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  drawer: { minHeight: 230 },
  detailsContent: { paddingBottom: 12 },
  pinTip: { borderLeftColor: 'transparent', borderLeftWidth: 8, borderRightColor: 'transparent', borderRightWidth: 8, borderTopColor: '#ef4444', borderTopWidth: 14, marginTop: -4 },
});
