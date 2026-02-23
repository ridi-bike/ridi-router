# Phase 6: Frontend Interactivity

## Overview

Add interactive features to the map visualization: popups showing element details when clicking points or lines, hover highlighting, and a legend explaining the color coding.

## Changes Required

### 1. Create PointPopup Component

**File**: `src/debug/rmdf_viewer/ui/src/components/PointPopup.tsx`

**Changes**: Create popup content for point details

```tsx
import { Popup } from 'react-leaflet';
import type { PointResponse } from '../types';

interface PointPopupProps {
  point: PointResponse | null;
  onClose: () => void;
}

export function PointPopup({ point, onClose }: PointPopupProps) {
  if (!point) return null;

  return (
    <Popup
      position={[point.lat, point.lon]}
      onClose={onClose}
    >
      <div style={{ minWidth: '200px' }}>
        <h4 style={{ margin: '0 0 0.5rem 0' }}>Point</h4>
        
        <table style={{ fontSize: '0.9rem', borderCollapse: 'collapse' }}>
          <tbody>
            <tr>
              <td style={{ paddingRight: '1rem', fontWeight: 'bold' }}>OSM ID:</td>
              <td>{point.osm_id}</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold' }}>Type:</td>
              <td>Point</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold' }}>Lat:</td>
              <td>{point.lat.toFixed(6)}</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold' }}>Lon:</td>
              <td>{point.lon.toFixed(6)}</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold' }}>Connected Lines:</td>
              <td>{point.connected_lines_count}</td>
            </tr>
            {point.flags.length > 0 && (
              <tr>
                <td style={{ fontWeight: 'bold', verticalAlign: 'top' }}>Flags:</td>
                <td>
                  {point.flags.map(flag => (
                    <span 
                      key={flag}
                      style={{ 
                        display: 'inline-block',
                        padding: '0.125rem 0.375rem',
                        marginRight: '0.25rem',
                        marginBottom: '0.25rem',
                        background: flag === 'nogo_area' ? '#ffcdd2' : 
                                   flag === 'residential_in_proximity' ? '#ffe0b2' : '#e0e0e0',
                        borderRadius: '4px',
                        fontSize: '0.8rem',
                      }}
                    >
                      {flag}
                    </span>
                  ))}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </Popup>
  );
}
```

**Rationale**: 
- Shows all point details: OSM ID, type, coordinates, connected lines, flags
- Flags displayed as colored badges matching map colors
- Table layout for clean readability

### 2. Create LinePopup Component

**File**: `src/debug/rmdf_viewer/ui/src/components/LinePopup.tsx`

**Changes**: Create popup content for line details

```tsx
import { Popup } from 'react-leaflet';
import type { LineResponse } from '../types';

interface LinePopupProps {
  line: LineResponse | null;
  onClose: () => void;
}

function formatDirection(direction: string): string {
  switch (direction) {
    case 'BothWays': return 'Both Ways';
    case 'OneWay': return 'One Way';
    case 'Roundabout': return 'Roundabout';
    default: return direction;
  }
}

export function LinePopup({ line, onClose }: LinePopupProps) {
  if (!line) return null;

  const midLat = (line.point_a[0] + line.point_b[0]) / 2;
  const midLon = (line.point_a[1] + line.point_b[1]) / 2;

  const hasTags = line.tags.name || line.tags.highway || 
                  line.tags.surface || line.tags.smoothness;

  return (
    <Popup
      position={[midLat, midLon]}
      onClose={onClose}
    >
      <div style={{ minWidth: '220px' }}>
        <h4 style={{ margin: '0 0 0.5rem 0' }}>Line</h4>
        
        <table style={{ fontSize: '0.9rem', borderCollapse: 'collapse' }}>
          <tbody>
            <tr>
              <td style={{ paddingRight: '1rem', fontWeight: 'bold' }}>Type:</td>
              <td>Line</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold' }}>Direction:</td>
              <td>{formatDirection(line.direction)}</td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold', verticalAlign: 'top' }}>Point A:</td>
              <td>
                <div>OSM: {line.point_a_osm_id}</div>
                <div style={{ fontSize: '0.8rem', color: '#666' }}>
                  {line.point_a[0].toFixed(5)}, {line.point_a[1].toFixed(5)}
                </div>
              </td>
            </tr>
            <tr>
              <td style={{ fontWeight: 'bold', verticalAlign: 'top' }}>Point B:</td>
              <td>
                <div>OSM: {line.point_b_osm_id}</div>
                <div style={{ fontSize: '0.8rem', color: '#666' }}>
                  {line.point_b[0].toFixed(5)}, {line.point_b[1].toFixed(5)}
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        
        {hasTags && (
          <>
            <h5 style={{ margin: '0.75rem 0 0.25rem 0', borderTop: '1px solid #ddd', paddingTop: '0.5rem' }}>
              Tags
            </h5>
            <table style={{ fontSize: '0.9rem', borderCollapse: 'collapse' }}>
              <tbody>
                {line.tags.name && (
                  <tr>
                    <td style={{ paddingRight: '1rem', fontWeight: 'bold' }}>Name:</td>
                    <td>{line.tags.name}</td>
                  </tr>
                )}
                {line.tags.highway && (
                  <tr>
                    <td style={{ fontWeight: 'bold' }}>Highway:</td>
                    <td>{line.tags.highway}</td>
                  </tr>
                )}
                {line.tags.surface && (
                  <tr>
                    <td style={{ fontWeight: 'bold' }}>Surface:</td>
                    <td>{line.tags.surface}</td>
                  </tr>
                )}
                {line.tags.smoothness && (
                  <tr>
                    <td style={{ fontWeight: 'bold' }}>Smoothness:</td>
                    <td>{line.tags.smoothness}</td>
                  </tr>
                )}
              </tbody>
            </table>
          </>
        )}
      </div>
    </Popup>
  );
}
```

**Rationale**: 
- Shows line details: direction, endpoints, tags
- Tags section only appears if at least one tag exists
- Coordinates formatted for readability
- Popup positioned at line midpoint

### 3. Update TileElements to Support Highlighting

**File**: `src/debug/rmdf_viewer/ui/src/components/TileElements.tsx`

**Changes**: Add hover highlighting support

```tsx
import { LayerGroup } from 'react-leaflet';
import type { LoadedTile } from '../hooks/useTiles';
import { PointLayer } from './PointLayer';
import { LineLayer } from './LineLayer';
import { DirectionArrows } from './DirectionArrows';
import type { PointResponse, LineResponse } from '../types';

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
      <LineLayer
        lines={data.lines}
        points={data.points}
        visible={visible}
        onLineClick={onLineClick}
        highlightedLineKey={highlightedLineKey}
        onLineHover={onLineHover}
      />
      
      <DirectionArrows
        lines={data.lines}
        visible={visible}
      />
      
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
```

### 4. Update PointLayer for Highlighting

**File**: `src/debug/rmdf_viewer/ui/src/components/PointLayer.tsx`

**Changes**: Add highlighting props

```tsx
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
```

**Rationale**: 
- Highlighted point: larger radius, black border, full opacity
- Hover events for highlighting

### 5. Update LineLayer for Highlighting

**File**: `src/debug/rmdf_viewer/ui/src/components/LineLayer.tsx`

**Changes**: Add highlighting props

```tsx
import { Polyline, LayerGroup } from 'react-leaflet';
import type { LineResponse, PointResponse } from '../types';
import { getLineColor } from '../utils/colors';

interface LineLayerProps {
  lines: LineResponse[];
  points: PointResponse[];
  visible: boolean;
  highlightedLineKey: string | null;
  onLineClick?: (line: LineResponse) => void;
  onLineHover?: (lineKey: string | null) => void;
}

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
        
        const pointAFlags = pointFlagsMap.get(line.point_a_osm_id) || [];
        const pointBFlags = pointFlagsMap.get(line.point_b_osm_id) || [];
        const color = getLineColor(pointAFlags, pointBFlags);
        
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
```

**Rationale**: 
- Highlighted line: black color, thicker, full opacity
- Line key combines endpoint OSM IDs for unique identification

### 6. Create Legend Component

**File**: `src/debug/rmdf_viewer/ui/src/components/Legend.tsx`

**Changes**: Create legend explaining color coding

```tsx
interface LegendProps {
  style?: React.CSSProperties;
}

const LEGEND_ITEMS = [
  { color: '#3388ff', label: 'Default (no flags)' },
  { color: '#ff9800', label: 'Residential in proximity' },
  { color: '#f44336', label: 'No-go area' },
];

const DIRECTION_ITEMS = [
  { symbol: '——', label: 'Both ways' },
  { symbol: '- -', label: 'One way (dashed)' },
  { symbol: '▶', label: 'Direction arrow' },
];

export function Legend({ style }: LegendProps) {
  return (
    <div 
      style={{ 
        padding: '0.75rem',
        background: 'rgba(255, 255, 255, 0.95)',
        borderRadius: '4px',
        boxShadow: '0 2px 6px rgba(0,0,0,0.2)',
        fontSize: '0.85rem',
        ...style,
      }}
    >
      <h4 style={{ margin: '0 0 0.5rem 0', fontSize: '0.9rem' }}>Color Legend</h4>
      
      <div style={{ marginBottom: '0.75rem' }}>
        {LEGEND_ITEMS.map(item => (
          <div 
            key={item.label}
            style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}
          >
            <span 
              style={{ 
                width: '16px', 
                height: '16px', 
                background: item.color, 
                borderRadius: '50%',
                border: '1px solid #333',
              }} 
            />
            <span>{item.label}</span>
          </div>
        ))}
      </div>
      
      <h4 style={{ margin: '0 0 0.5rem 0', fontSize: '0.9rem' }}>Line Styles</h4>
      <div>
        {DIRECTION_ITEMS.map(item => (
          <div 
            key={item.label}
            style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.25rem' }}
          >
            <span 
              style={{ 
                width: '16px', 
                textAlign: 'center',
                fontFamily: 'monospace',
                fontSize: '0.8rem',
              }} 
            >
              {item.symbol}
            </span>
            <span>{item.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

**Rationale**: 
- Shows color meanings for flag-based coloring
- Explains line styles (solid vs dashed, arrows)
- Positioned in map corner

### 7. Update App with Popups and Legend

**File**: `src/debug/rmdf_viewer/ui/src/App.tsx`

**Changes**: Wire up popups, highlighting, and legend. Add highlighted state for PointLayer and LineLayer. Position Legend in bottom-right corner over map. Popups at element location.

## Success Criteria

### Automated Verification:
- [x] `bun run build` compiles without errors
- [x] TypeScript: no type errors

### Manual Verification:
- [ ] Hover over point - point enlarges with black border
- [ ] Hover over line - line thickens and turns black
- [ ] Click point - popup appears with OSM ID, coords, flags, connected lines
- [ ] Click line - popup appears with endpoints, direction, tags
- [ ] Flag badges in popup match map colors
- [ ] Tags section shows name, highway, surface, smoothness if available
- [ ] Close popup - popup disappears
- [ ] Legend shows color meanings
- [ ] Legend shows line style meanings
- [ ] Legend visible in bottom-right corner

## Dependencies

- Depends on: Phase 5 (Frontend Map Visualization)
- Blocks: Phase 7 (Asset Bundling)

## Out of Scope for This Phase

- Asset bundling into binary - Phase 7
- Build configuration - Phase 7
- Backend changes - all backend work complete
- URL state persistence - out of scope for entire project

## Notes

- The popup uses Leaflet's built-in Popup component which auto-pans to stay visible
- Line popup is positioned at the midpoint of the line segment
- Highlighting state is separate from selection state - hover highlights, click selects

## Deviations from Plan

### Phase 6: Frontend Interactivity
- **Original Plan**: Popup component using `onClose` prop for handling popup close events
- **Actual Implementation**: Used `eventHandlers={{ remove: onClose }}` instead
- **Reason for Deviation**: react-leaflet's Popup component does not have an `onClose` prop. The correct way to handle popup close events is through the `remove` event handler
- **Impact Assessment**: None - functionality is identical, just uses the correct API
- **Date/Time**: 2026-02-23
