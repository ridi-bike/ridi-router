import { useEffect } from 'react';
import { useMap } from 'react-leaflet';
import L from 'leaflet';
import type { TileSummary } from '../types';

interface TileBordersProps {
  tiles: TileSummary[];
  onTileClick: (filename: string) => void;
}

export function TileBorders({ tiles, onTileClick }: TileBordersProps) {
  const map = useMap();

  useEffect(() => {
    const borders: L.Rectangle[] = [];
    const centerMarkers: L.CircleMarker[] = [];

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
      
      // Add center marker for the tile
      const centerLat = (lat_min + lat_max) / 2;
      const centerLon = (lon_min + lon_max) / 2;
      
      const centerMarker = L.circleMarker([centerLat, centerLon], {
        radius: 8,
        color: '#fff',
        weight: 2,
        fillColor: '#3388ff',
        fillOpacity: 0.9,
      });
      
      // Show tooltip on hover
      centerMarker.bindTooltip(tile.filename, {
        permanent: false,
        direction: 'top',
        offset: [0, -10],
      });
      
      // Handle click to scroll to tile in list
      centerMarker.on('click', () => {
        onTileClick(tile.filename);
      });
      
      centerMarker.addTo(map);
      centerMarkers.push(centerMarker);
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
      centerMarkers.forEach(marker => marker.remove());
    };
  }, [map, tiles, onTileClick]);

  return null;
}
