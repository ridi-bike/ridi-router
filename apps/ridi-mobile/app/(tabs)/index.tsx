import { Camera, Images, Map } from '@maplibre/maplibre-react-native';
import { StyleSheet, View } from 'react-native';

import { ridiMapStyle } from '@/map/ridiMapStyle';

export default function RideScreen() {
  return (
    <View className="flex-1 bg-background">
      <Map
        attribution={false}
        compass={false}
        logo={false}
        mapStyle={ridiMapStyle}
        style={StyleSheet.absoluteFill}
      >
        <Images
          images={{
            'military-hatch': require('@/assets/map/pattern-military-hatch.png'),
            'park-dot': require('@/assets/map/pattern-park-dot.png'),
          }}
        />
        <Camera initialViewState={{ center: [11.5761, 48.1372], zoom: 10 }} />
      </Map>
    </View>
  );
}
