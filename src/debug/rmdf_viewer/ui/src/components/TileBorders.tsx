import { useEffect } from 'react';
import { useMap } from 'react-leaflet';
import L from 'leaflet';
import type { TileSummary } from '../types';

interface TileBordersProps {
  tiles: TileSummary[];
}

export function TileBorders({ tiles }: TileBordersProps) {
  const map = useMap();

  useEffect(() => {
    const borders: L.Rectangle[] = [];

    tiles.forEach(tile => {
      const { lat_min, lat_max, lon_min, lon_max } = tile.bounds;
      
      const bounds: L.LatLngBoundsExpression = [
        [lat_min, lon_min],
        [lat_max, lon_max],
      ];
      
      const rect = L.rectangle(bounds, {
        color: '#3388ff',
        weight: 1,
        fill: false,
        dashArray: '5, 5',
      });
      
      rect.bindTooltip(tile.filename, {
        permanent: false,
        direction: 'center',
      });
      
      rect.addTo(map);
      borders.push(rect);
    });

    // Fit map to show all tiles
    if (tiles.length > 0) {
      const allBounds = L.latLngBounds(
        tiles.flatMap(t => [
          [t.bounds.lat_min, t.bounds.lon_min],
          [t.bounds.lat_max, t.bounds.lon_max],
        ])
      );
      map.fitBounds(allBounds, { padding: [20, 20] });
    }

    return () => {
      borders.forEach(rect => rect.remove());
    };
  }, [map, tiles]);

  return null;
}
