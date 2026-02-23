import { LayerGroup } from 'react-leaflet';
import type { LoadedTile } from '../hooks/useTiles';
import type { PointResponse, LineResponse } from '../types';
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
  const { data, visible } = tile;

  return (
    <LayerGroup>
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
