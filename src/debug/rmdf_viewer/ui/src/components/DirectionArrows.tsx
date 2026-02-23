import { LayerGroup, Marker } from 'react-leaflet';
import L from 'leaflet';
import type { LineResponse } from '../types';
import { COLORS } from '../utils/colors';

interface DirectionArrowsProps {
  lines: LineResponse[];
  visible: boolean;
}

/**
 * Calculate midpoint between two coordinates
 */
function midpoint(lat1: number, lon1: number, lat2: number, lon2: number): [number, number] {
  return [(lat1 + lat2) / 2, (lon1 + lon2) / 2];
}

/**
 * Calculate bearing (heading) from point A to point B in degrees
 */
function calculateBearing(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const lat1Rad = (lat1 * Math.PI) / 180;
  const lat2Rad = (lat2 * Math.PI) / 180;
  
  const y = Math.sin(dLon) * Math.cos(lat2Rad);
  const x = Math.cos(lat1Rad) * Math.sin(lat2Rad) -
            Math.sin(lat1Rad) * Math.cos(lat2Rad) * Math.cos(dLon);
  
  let bearing = (Math.atan2(y, x) * 180) / Math.PI;
  return (bearing + 360) % 360;
}

export function DirectionArrows({ lines, visible }: DirectionArrowsProps) {
  if (!visible) return null;

  const oneWayLines = lines.filter(l => l.direction === 'OneWay');

  return (
    <LayerGroup>
      {oneWayLines.map((line, index) => {
        const [midLat, midLon] = midpoint(
          line.point_a[0], line.point_a[1],
          line.point_b[0], line.point_b[1]
        );
        
        const bearing = calculateBearing(
          line.point_a[0], line.point_a[1],
          line.point_b[0], line.point_b[1]
        );
        
        // Create rotated arrow icon
        const rotatedIcon = L.divIcon({
          html: `<svg width="16" height="16" viewBox="0 0 16 16" style="transform: rotate(${bearing}deg);">
            <path d="M8 2 L14 14 L8 10 L2 14 Z" fill="${COLORS.arrow}" />
          </svg>`,
          className: 'direction-arrow',
          iconSize: [16, 16],
          iconAnchor: [8, 8],
        });
        
        return (
          <Marker
            key={`arrow-${line.point_a_osm_id}-${line.point_b_osm_id}-${index}`}
            position={[midLat, midLon]}
            icon={rotatedIcon}
            interactive={false}
          />
        );
      })}
    </LayerGroup>
  );
}
