import { LayerGroup } from 'react-leaflet';
import type { LoadedTile } from '../hooks/useTiles';
import type { PointResponse, LineResponse } from '../types';
import { PointLayer } from './PointLayer';
import { LineLayer } from './LineLayer';
import { DirectionArrows } from './DirectionArrows';

interface TileElementsProps {
  tile: LoadedTile;
  onPointClick?: (point: PointResponse) => void;
  onLineClick?: (line: LineResponse) => void;
}

export function TileElements({ tile, onPointClick, onLineClick }: TileElementsProps) {
  const { data, visible } = tile;

  return (
    <LayerGroup>
      {/* Lines first (behind points) */}
      <LineLayer
        lines={data.lines}
        points={data.points}
        visible={visible}
        onLineClick={onLineClick}
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
      />
    </LayerGroup>
  );
}
