# Phase 5: Frontend Map Visualization

## Overview

Implement the map visualization that renders points as circle markers and lines as polylines. This phase adds color coding based on element flags, direction arrows for one-way lines, and connects the visibility toggle to actually show/hide elements.

## Changes Required

### 1. Create Color Utilities

**File**: `src/debug/rmdf_viewer/ui/src/utils/colors.ts`

**Changes**: Define color scheme for element flags

```typescript
// Color scheme for elements based on flags
export const COLORS = {
  // Default colors
  point: '#3388ff',      // Blue
  line: '#3388ff',       // Blue
  
  // Flag-based colors (take precedence)
  residentialInProximity: '#ff9800',  // Orange
  nogoArea: '#f44336',                // Red
  
  // Direction arrow color
  arrow: '#333333',
} as const;

/**
 * Get color for a point based on its flags
 */
export function getPointColor(flags: string[]): string {
  if (flags.includes('nogo_area')) {
    return COLORS.nogoArea;
  }
  if (flags.includes('residential_in_proximity')) {
    return COLORS.residentialInProximity;
  }
  return COLORS.point;
}

/**
 * Get color for a line based on connected point flags
 * Lines inherit the most severe flag from their endpoints
 */
export function getLineColor(pointAFlags: string[], pointBFlags: string[]): string {
  // Check for nogo_area first (most severe)
  if (pointAFlags.includes('nogo_area') || pointBFlags.includes('nogo_area')) {
    return COLORS.nogoArea;
  }
  // Then residential_in_proximity
  if (pointAFlags.includes('residential_in_proximity') || 
      pointBFlags.includes('residential_in_proximity')) {
    return COLORS.residentialInProximity;
  }
  return COLORS.line;
}
```

**Rationale**: 
- Clear color hierarchy: nogo_area (red) > residential_in_proximity (orange) > default (blue)
- Lines colored by most severe flag of their endpoints
- Centralized color definitions for consistency

### 2. Create PointLayer Component

**File**: `src/debug/rmdf_viewer/ui/src/components/PointLayer.tsx`

**Changes**: Render points as circle markers

```tsx
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
```

**Rationale**: 
- Uses CircleMarker for consistent size regardless of zoom
- LayerGroup for efficient batch operations
- Small radius (4px) suitable for dense point clusters
- Click handler for popup (Phase 6)

### 3. Create LineLayer Component

**File**: `src/debug/rmdf_viewer/ui/src/components/LineLayer.tsx`

**Changes**: Render lines as polylines with direction arrows

```tsx
import { Polyline, LayerGroup, Tooltip } from 'react-leaflet';
import type { LineResponse, PointResponse } from '../types';
import { getLineColor } from '../utils/colors';

interface LineLayerProps {
  lines: LineResponse[];
  points: PointResponse[];  // For flag lookup
  visible: boolean;
  onLineClick?: (line: LineResponse) => void;
}

/**
 * Build a map of OSM ID to point for quick flag lookup
 */
function buildPointFlagsMap(points: PointResponse[]): Map<number, string[]> {
  const map = new Map<number, string[]>();
  points.forEach(p => map.set(p.osm_id, p.flags));
  return map;
}

export function LineLayer({ lines, points, visible, onLineClick }: LineLayerProps) {
  if (!visible) return null;

  const pointFlagsMap = buildPointFlagsMap(points);

  return (
    <LayerGroup>
      {lines.map((line, index) => {
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
            key={`${line.point_a_osm_id}-${line.point_b_osm_id}-${index}`}
            positions={positions}
            pathOptions={{
              color,
              weight: 3,
              opacity: 0.8,
              // Dashed line for one-way
              dashArray: line.direction === 'OneWay' ? '10, 5' : undefined,
            }}
            eventHandlers={{
              click: () => onLineClick?.(line),
            }}
          />
        );
      })}
    </LayerGroup>
  );
}
```

**Rationale**: 
- Uses Polyline for line segments
- Builds point flags map for efficient lookup
- Dashed line for one-way (visual distinction before arrows)
- Click handler for popup (Phase 6)

### 4. Create DirectionArrow Component

**File**: `src/debug/rmdf_viewer/ui/src/components/DirectionArrows.tsx`

**Changes**: Add direction arrows for one-way lines

```tsx
import { LayerGroup, Marker } from 'react-leaflet';
import L from 'leaflet';
import type { LineResponse } from '../types';
import { COLORS } from '../utils/colors';

interface DirectionArrowsProps {
  lines: LineResponse[];
  visible: boolean;
}

// Create arrow icon once
const arrowIcon = L.divIcon({
  html: `<svg width="16" height="16" viewBox="0 0 16 16" style="transform: rotate(0deg);">
    <path d="M8 2 L14 14 L8 10 L2 14 Z" fill="${COLORS.arrow}" />
  </svg>`,
  className: 'direction-arrow',
  iconSize: [16, 16],
  iconAnchor: [8, 8],
});

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
```

**Rationale**: 
- Arrow at midpoint of one-way lines
- Bearing calculation for correct rotation
- SVG arrow with rotation transform
- Non-interactive (doesn't block line clicks)

### 5. Create TileElements Component

**File**: `src/debug/rmdf_viewer/ui/src/components/TileElements.tsx`

**Changes**: Combine point and line layers for a single tile

```tsx
import { LayerGroup } from 'react-leaflet';
import type { LoadedTile } from '../hooks/useTiles';
import { PointLayer } from './PointLayer';
import { LineLayer } from './LineLayer';
import { DirectionArrows } from './DirectionArrows';

interface TileElementsProps {
  tile: LoadedTile;
  onPointClick?: (point: typeof tile.data.points[0]) => void;
  onLineClick?: (line: typeof tile.data.lines[0]) => void;
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
```

**Rationale**: 
- Combines all layers for a single tile
- Proper z-order: lines < arrows < points
- Visibility toggle affects all layers

### 6. Update App Component

**File**: `src/debug/rmdf_viewer/ui/src/App.tsx`

**Changes**: Add TileElements for each loaded tile

```tsx
import { useEffect, useState } from 'react';
import { MapContainer, TileLayer } from 'react-leaflet';
import 'leaflet/dist/leaflet.css';

import { useTiles, type LoadedTile } from './hooks/useTiles';
import { TileList } from './components/TileList';
import { TileBorders } from './components/TileBorders';
import { TileElements } from './components/TileElements';
import type { PointResponse, LineResponse } from '../types';

function App() {
  const { 
    manifest, 
    loadedTiles, 
    loading, 
    error, 
    loadManifest, 
    loadTile, 
    toggleVisibility 
  } = useTiles();

  const [selectedPoint, setSelectedPoint] = useState<PointResponse | null>(null);
  const [selectedLine, setSelectedLine] = useState<LineResponse | null>(null);

  useEffect(() => {
    loadManifest();
  }, [loadManifest]);

  return (
    <div style={{ display: 'flex', height: '100vh' }}>
      <aside style={{ width: '300px', padding: '1rem', background: '#f5f5f5', overflow: 'hidden' }}>
        <h2 style={{ margin: '0 0 1rem 0' }}>RMDF Debug Viewer</h2>
        
        {error && (
          <div style={{ padding: '0.5rem', background: '#ffebee', color: '#c62828', marginBottom: '1rem' }}>
            {error}
          </div>
        )}
        
        {manifest ? (
          <TileList
            tiles={manifest.tiles}
            loadedTiles={loadedTiles}
            onLoadTile={loadTile}
            onToggleVisibility={toggleVisibility}
            loading={loading}
          />
        ) : (
          <p>{loading ? 'Loading manifest...' : 'No manifest loaded'}</p>
        )}
      </aside>

      <main style={{ flex: 1 }}>
        <MapContainer
          center={[0, 0]}
          zoom={2}
          style={{ height: '100%', width: '100%' }}
        >
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
            url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
          />
          
          {manifest && <TileBorders tiles={manifest.tiles} />}
          
          {/* Render elements for each loaded tile */}
          {Array.from(loadedTiles.values()).map(tile => (
            <TileElements
              key={tile.summary.filename}
              tile={tile}
              onPointClick={setSelectedPoint}
              onLineClick={setSelectedLine}
            />
          ))}
        </MapContainer>
      </main>
    </div>
  );
}

export default App;
```

**Rationale**: 
- Iterates over loaded tiles to render each one
- Click handlers set selected point/line state (for Phase 6 popups)
- Visibility toggle now actually shows/hides elements

## Success Criteria

### Automated Verification:
- [ ] `bun run build` compiles without errors
- [ ] TypeScript: no type errors

### Manual Verification:
- [ ] Load a tile - points appear as blue circles
- [ ] Load a tile - lines appear as blue polylines
- [ ] Points with `residential_in_proximity` flag are orange
- [ ] Points with `nogo_area` flag are red
- [ ] Lines connected to flagged points show corresponding colors
- [ ] One-way lines appear dashed
- [ ] One-way lines have direction arrows at midpoint
- [ ] Arrow direction matches line direction (A to B)
- [ ] Toggle visibility off - elements disappear
- [ ] Toggle visibility on - elements reappear
- [ ] Load multiple tiles - all render correctly
- [ ] No performance issues with 1000+ points

## Dependencies

- Depends on: Phase 4 (Frontend Tile Management)
- Blocks: Phase 6 (Frontend Interactivity)

## Risks & Mitigations

- **Risk**: Large tiles (5000+ points) may slow rendering
  - **Mitigation**: Leaflet handles this reasonably well; can optimize with simpler markers if needed

- **Risk**: Direction arrows may overlap on dense areas
  - **Mitigation**: Acceptable for debug tool; arrows are small

## Out of Scope for This Phase

**CRITICAL**: The following items are explicitly NOT part of this phase:
- Popups on click - Phase 6
- Hover highlighting - Phase 6
- Legend component - Phase 6
- Connected lines visualization - Phase 6
- Actual popup content rendering

## Phase Boundary Rules

**IMPORTANT**: When executing this phase:
1. **Focus on rendering only** - Points and lines on map
2. **Click handlers are stubs** - They set state but nothing uses it yet
3. **No popups** - That's Phase 6

## Notes

- Color hierarchy: nogo_area > residential_in_proximity > default
- Lines are colored by their endpoints' most severe flag
- Direction arrows use SVG with CSS rotation for simplicity
- The bearing calculation handles the lat/lon to screen rotation correctly
