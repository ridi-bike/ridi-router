import { Polyline, LayerGroup } from 'react-leaflet';
import type { LineResponse, PointResponse } from '../types';
import { getLineColor } from '../utils/colors';

interface LineLayerProps {
  lines: LineResponse[];
  points: PointResponse[];  // For flag lookup
  visible: boolean;
  highlightedLineKey: string | null;
  onLineClick?: (line: LineResponse) => void;
  onLineHover?: (lineKey: string | null) => void;
}

/**
 * Build a map of OSM ID to point for quick flag lookup
 */
function buildPointFlagsMap(points: PointResponse[]): Map<number, string[]> {
  const map = new Map<number, string[]>();
  points.forEach(p => map.set(p.osm_id, p.flags));
  return map;
}

function makeLineKey(line: LineResponse): string {
  return `${line.point_a_osm_id}-${line.point_b_osm_id}`;
}

export function LineLayer({ 
  lines, 
  points, 
  visible, 
  highlightedLineKey,
  onLineClick,
  onLineHover,
}: LineLayerProps) {
  if (!visible) return null;

  const pointFlagsMap = buildPointFlagsMap(points);

  return (
    <LayerGroup>
      {lines.map((line, index) => {
        const lineKey = makeLineKey(line);
        const isHighlighted = lineKey === highlightedLineKey;
        
        // Get flags for endpoints (empty array if point not found)
        const pointAFlags = pointFlagsMap.get(line.point_a_osm_id) || [];
        const pointBFlags = pointFlagsMap.get(line.point_b_osm_id) || [];
        const color = getLineColor(pointAFlags, pointBFlags);
        
        // Polyline coordinates: Leaflet uses [lat, lon]
        const positions: [number, number][] = [
          [line.point_a[0], line.point_a[1]],
          [line.point_b[0], line.point_b[1]],
        ];
        
        return (
          <Polyline
            key={`${lineKey}-${index}`}
            positions={positions}
            pathOptions={{
              color: isHighlighted ? '#000' : color,
              weight: isHighlighted ? 6 : 3,
              opacity: isHighlighted ? 1 : 0.8,
              // Dashed line for one-way
              dashArray: line.direction === 'OneWay' ? '10, 5' : undefined,
            }}
            eventHandlers={{
              click: () => onLineClick?.(line),
              mouseover: () => onLineHover?.(lineKey),
              mouseout: () => onLineHover?.(null),
            }}
          />
        );
      })}
    </LayerGroup>
  );
}
