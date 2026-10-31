import { Camera, Images, Map, Marker, type MapProps } from '@maplibre/maplibre-react-native';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { ridiMapStyle } from '@/map/ridiMapStyle';

type SelectedCoordinate = {
  latitude: number;
  longitude: number;
};

export default function RideScreen() {
  const [selectedCoordinate, setSelectedCoordinate] = useState<SelectedCoordinate | null>(null);

  const handleLongPress: NonNullable<MapProps['onLongPress']> = (event) => {
    const [longitude, latitude] = event.nativeEvent.lngLat;
    setSelectedCoordinate({ latitude, longitude });
  };
  return (
    <View className="flex-1 bg-background">
      <Map
        attribution={false}
        compass={false}
        logo={false}
        mapStyle={ridiMapStyle}
        onLongPress={handleLongPress}
        style={StyleSheet.absoluteFill}
      >
        <Images
          images={{
            'military-hatch': require('@/assets/map/pattern-military-hatch.png'),
            'park-dot': require('@/assets/map/pattern-park-dot.png'),
          }}
        />
        <Camera initialViewState={{ center: [11.5761, 48.1372], zoom: 10 }} />
        {selectedCoordinate ? (
          <Marker
            anchor="bottom"
            id="selected-coordinate"
            lngLat={[selectedCoordinate.longitude, selectedCoordinate.latitude]}
          >
            <View className="items-center">
              <View className="mb-2 rounded-xl border border-border bg-card/95 px-3 py-2 shadow-sm">
                <Text className="text-foreground text-xs font-semibold">Selected GPS</Text>
                <Text className="text-foreground text-xs">
                  {selectedCoordinate.latitude.toFixed(6)}, {selectedCoordinate.longitude.toFixed(6)}
                </Text>
              </View>
              <View className="h-5 w-5 items-center justify-center rounded-full bg-primary/25">
                <View className="h-3 w-3 rounded-full border-2 border-card bg-primary" />
              </View>
            </View>
          </Marker>
        ) : null}
      </Map>
    </View>
  );
}
