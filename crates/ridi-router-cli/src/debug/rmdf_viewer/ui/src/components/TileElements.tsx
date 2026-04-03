import { LayerGroup, Polygon as LeafletPolygon } from 'react-leaflet';
import type { LoadedTile } from '../hooks/useTiles';
import type { LineResponse, PointResponse } from '../types';
import { PointLayer } from './PointLayer';
import { LineLayer } from './LineLayer';
import { DirectionArrows } from './DirectionArrows';

interface TileElementsProps {
  tile: LoadedTile;
  highlightedPointId: number | null;
  highlightedLineKey: string | null;
  onPointClick?: (point: PointResponse) => void;
  onLineClick?: (line: LineResponse) => void;
  onPointHover?: (pointId: number | null) => void;
  onLineHover?: (lineKey: string | null) => void;
}

export function TileElements({
  tile,
  highlightedPointId,
  highlightedLineKey,
  onPointClick,
  onLineClick,
  onPointHover,
  onLineHover,
}: TileElementsProps) {
  const { data, militaryGeoJson, visible } = tile;

  const militaryPolygonPositions = (militaryGeoJson?.features ?? []).flatMap((feature, featureIndex) =>
    feature.geometry.coordinates.map((polygonCoords, polygonIndex) => ({
      key: `${featureIndex}-${polygonIndex}`,
      positions: polygonCoords.map(ring => ring.map(([lon, lat]) => [lat, lon] as [number, number])),
    })),
  );

  return (
    <LayerGroup>
      {visible && militaryPolygonPositions.map(polygon => (
        <LeafletPolygon
          key={polygon.key}
          positions={polygon.positions}
          pathOptions={{
            color: '#c2185b',
            weight: 2,
            fillColor: '#e91e63',
            fillOpacity: 0.12,
          }}
        />
      ))}

      {/* Lines first (behind points) */}
      <LineLayer
        lines={data.lines}
        points={data.points}
        visible={visible}
        onLineClick={onLineClick}
        highlightedLineKey={highlightedLineKey}
        onLineHover={onLineHover}
      />

      {/* Direction arrows for one-way lines */}
      <DirectionArrows
        lines={data.lines}
        visible={visible}
      />

      {/* Points on top */}
      <PointLayer
        points={data.points}
        visible={visible}
        onPointClick={onPointClick}
        highlightedPointId={highlightedPointId}
        onPointHover={onPointHover}
      />
    </LayerGroup>
  );
}
