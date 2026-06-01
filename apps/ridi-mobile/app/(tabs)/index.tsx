import { Camera, Images, Map, Marker, type MapProps, type MapRef } from '@maplibre/maplibre-react-native';
import { useEffect, useRef, useState } from 'react';
import { Animated, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { INTERACTIVE_MAP_LAYER_IDS, ridiMapStyle } from '@/map/ridiMapStyle';

type SelectedFeatureKind = 'coordinate' | 'poi' | 'name';

type SelectedPoint = {
  latitude: number;
  longitude: number;
  kind: SelectedFeatureKind;
  name?: string;
  details: DetailRow[];
};

type DetailRow = {
  label: string;
  value: string;
};

type FeatureLike = {
  geometry?: {
    type?: string;
    coordinates?: unknown;
  } | null;
  layer?: { id?: string };
  properties?: Record<string, unknown> | null;
};

const DETAIL_FIELDS = ['kind', 'kind_detail', 'cuisine', 'sport', 'religion', 'shop', 'tourism', 'amenity'] as const;

function stringValue(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim().length > 0) {
    return value;
  }

  if (typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }

  return undefined;
}

function featureName(properties: Record<string, unknown>): string | undefined {
  return stringValue(properties['name:en']) ?? stringValue(properties.name);
}

function detailRows(properties: Record<string, unknown>, fields: readonly string[]): DetailRow[] {
  return fields.flatMap((field) => {
    const value = stringValue(properties[field]);
    return value ? [{ label: field.replaceAll('_', ' '), value }] : [];
  });
}

function selectedFeatureDetails(feature: FeatureLike | undefined): Pick<SelectedPoint, 'kind' | 'name' | 'details'> {
  if (!feature?.layer?.id || !feature.properties) {
    return { kind: 'coordinate', details: [] };
  }

  const name = featureName(feature.properties);

  if (feature.layer.id === 'poi_names' || feature.layer.id === 'poi_dots') {
    return {
      kind: 'poi',
      name,
      details: detailRows(feature.properties, DETAIL_FIELDS),
    };
  }

  return {
    kind: 'name',
    name,
    details: detailRows(feature.properties, ['kind', 'kind_detail']),
  };
}

function selectedFeatureCoordinate(feature: FeatureLike | undefined): Pick<SelectedPoint, 'latitude' | 'longitude'> | undefined {
  if (feature?.geometry?.type !== 'Point' || !Array.isArray(feature.geometry.coordinates)) {
    return undefined;
  }

  const [longitude, latitude] = feature.geometry.coordinates;

  if (typeof latitude !== 'number' || typeof longitude !== 'number') {
    return undefined;
  }

  return { latitude, longitude };
}

function drawerTitle(selectedPoint: SelectedPoint): string {
  if (selectedPoint.name) {
    return selectedPoint.name;
  }

  if (selectedPoint.kind === 'poi') {
    return 'Selected POI';
  }

  if (selectedPoint.kind === 'name') {
    return 'Selected place';
  }

  return 'Selected point';
}

export default function RideScreen() {
  const insets = useSafeAreaInsets();
  const mapRef = useRef<MapRef>(null);
  const [drawerTranslateY] = useState(() => new Animated.Value(320));
  const [selectedPoint, setSelectedPoint] = useState<SelectedPoint | null>(null);

  useEffect(() => {
    Animated.timing(drawerTranslateY, {
      duration: 220,
      toValue: selectedPoint ? 0 : 320,
      useNativeDriver: true,
    }).start();
  }, [drawerTranslateY, selectedPoint]);

  const handleMapPress: NonNullable<MapProps['onPress']> = async (event) => {
    const [longitude, latitude] = event.nativeEvent.lngLat;
    const features = await mapRef.current?.queryRenderedFeatures(event.nativeEvent.point, {
      layers: [...INTERACTIVE_MAP_LAYER_IDS],
    });
    const feature = features?.[0] as FeatureLike | undefined;
    const selectedDetails = selectedFeatureDetails(feature);
    const selectedCoordinate = selectedFeatureCoordinate(feature) ?? { latitude, longitude };

    setSelectedPoint({
      ...selectedCoordinate,
      ...selectedDetails,
    });
  };

  return (
    <View className="flex-1 bg-background">
      <Map
        ref={mapRef}
        attribution={false}
        compass={false}
        logo={false}
        mapStyle={ridiMapStyle}
        onPress={handleMapPress}
        style={StyleSheet.absoluteFill}
      >
        <Images
          images={{
            'military-hatch': require('@/assets/map/pattern-military-hatch.png'),
            'park-dot': require('@/assets/map/pattern-park-dot.png'),
          }}
        />
        <Camera initialViewState={{ center: [11.5761, 48.1372], zoom: 10 }} />
        {selectedPoint ? (
          <Marker anchor="bottom" id="selected-point" lngLat={[selectedPoint.longitude, selectedPoint.latitude]}>
            <View className="items-center">
              <View className="h-10 w-10 items-center justify-center rounded-full border-2 border-white bg-red-500 shadow-lg">
                <View className="h-3 w-3 rounded-full bg-neutral-900" />
              </View>
              <View style={styles.pinTip} />
            </View>
          </Marker>
        ) : null}
      </Map>

      <Animated.View
        pointerEvents={selectedPoint ? 'auto' : 'none'}
        className="absolute bottom-0 left-0 right-0 rounded-t-[28px] bg-neutral-950 px-5 pt-3 shadow-2xl"
        style={[
          styles.drawer,
          {
            paddingBottom: insets.bottom + 20,
            transform: [{ translateY: drawerTranslateY }],
          },
        ]}
      >
        {selectedPoint ? (
          <>
            <View className="mb-5 items-center">
              <View className="h-1.5 w-20 rounded-full bg-neutral-500" />
            </View>

            <View className="flex-row items-start justify-between gap-4">
              <View className="flex-1">
                <Text className="text-3xl font-semibold text-white">{drawerTitle(selectedPoint)}</Text>
                <Text className="mt-3 text-base text-neutral-300">
                  {selectedPoint.latitude.toFixed(6)}, {selectedPoint.longitude.toFixed(6)}
                </Text>
                <Text className="mt-2 text-sm uppercase tracking-wider text-neutral-500">{selectedPoint.kind}</Text>
              </View>

              <Pressable
                accessibilityRole="button"
                accessibilityLabel="Close selected point details"
                className="h-12 w-12 items-center justify-center rounded-full bg-neutral-800"
                onPress={() => setSelectedPoint(null)}
              >
                <Text className="text-4xl leading-10 text-neutral-200">×</Text>
              </Pressable>
            </View>

            {selectedPoint.details.length > 0 ? (
              <View className="mt-5 gap-2">
                {selectedPoint.details.map((detail) => (
                  <View className="flex-row justify-between gap-4" key={detail.label}>
                    <Text className="text-base capitalize text-neutral-400">{detail.label}</Text>
                    <Text className="flex-1 text-right text-base text-neutral-100">{detail.value}</Text>
                  </View>
                ))}
              </View>
            ) : null}
          </>
        ) : null}
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  drawer: {
    minHeight: 230,
  },
  pinTip: {
    borderLeftColor: 'transparent',
    borderLeftWidth: 8,
    borderRightColor: 'transparent',
    borderRightWidth: 8,
    borderTopColor: '#ef4444',
    borderTopWidth: 14,
    marginTop: -4,
  },
});
