import { CircleMarker, LayerGroup } from 'react-leaflet';
import type { PointResponse } from '../types';
import { getPointColor } from '../utils/colors';

interface PointLayerProps {
  points: PointResponse[];
  visible: boolean;
  onPointClick?: (point: PointResponse) => void;
}

export function PointLayer({ points, visible, onPointClick }: PointLayerProps) {
  if (!visible) return null;

  return (
    <LayerGroup>
      {points.map(point => (
        <CircleMarker
          key={point.osm_id}
          center={[point.lat, point.lon]}
          radius={4}
          pathOptions={{
            color: getPointColor(point.flags),
            fillColor: getPointColor(point.flags),
            fillOpacity: 0.8,
            weight: 1,
          }}
          eventHandlers={{
            click: () => onPointClick?.(point),
          }}
        />
      ))}
    </LayerGroup>
  );
}
