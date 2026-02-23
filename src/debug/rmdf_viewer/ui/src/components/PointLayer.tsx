import { CircleMarker, LayerGroup } from 'react-leaflet';
import type { PointResponse } from '../types';
import { getPointColor } from '../utils/colors';


interface PointLayerProps {
  points: PointResponse[];
  visible: boolean;
  highlightedPointId: number | null;
  onPointClick?: (point: PointResponse) => void;
  onPointHover?: (pointId: number | null) => void;
}


export function PointLayer({ 
  points, 
  visible, 
  highlightedPointId,
  onPointClick,
  onPointHover,
}: PointLayerProps) {
  if (!visible) return null;

  return (
    <LayerGroup>
      {points.map(point => {
        const isHighlighted = point.osm_id === highlightedPointId;
        const color = getPointColor(point.flags);
        
        return (
          <CircleMarker
            key={point.osm_id}
            center={[point.lat, point.lon]}
            radius={isHighlighted ? 8 : 4}
            pathOptions={{
              color: isHighlighted ? '#000' : color,
              fillColor: color,
              fillOpacity: isHighlighted ? 1 : 0.8,
              weight: isHighlighted ? 3 : 1,
            }}
            eventHandlers={{
              click: () => onPointClick?.(point),
              mouseover: () => onPointHover?.(point.osm_id),
              mouseout: () => onPointHover?.(null),
            }}
          />
        );
      })}
    </LayerGroup>
  );
}
