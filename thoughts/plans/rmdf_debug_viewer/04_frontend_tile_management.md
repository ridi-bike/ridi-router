# Phase 4: Frontend Tile Management

## Overview

Implement the tile list component that displays available tiles from the manifest, draws tile borders on the map, and allows users to load tiles on demand. This phase connects the frontend to the backend API and enables tile selection.

## Changes Required

### 1. Create Tile State Management

**File**: `src/debug/rmdf_viewer/ui/src/hooks/useTiles.ts`

**Changes**: Create custom hook for tile state management

```typescript
import { useState, useCallback } from 'react';
import type { ManifestResponse, TileSummary, TileResponse } from '../types';
import { fetchManifest, fetchTile } from '../api/client';

export interface LoadedTile {
  summary: TileSummary;
  data: TileResponse;
  visible: boolean;
}

export function useTiles() {
  const [manifest, setManifest] = useState<ManifestResponse | null>(null);
  const [loadedTiles, setLoadedTiles] = useState<Map<string, LoadedTile>>(new Map());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadManifest = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchManifest();
      setManifest(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load manifest');
    } finally {
      setLoading(false);
    }
  }, []);

  const loadTile = useCallback(async (filename: string) => {
    if (loadedTiles.has(filename)) return; // Already loaded
    
    setLoading(true);
    setError(null);
    try {
      const data = await fetchTile(filename);
      const summary = manifest?.tiles.find(t => t.filename === filename);
      if (!summary) throw new Error('Tile not found in manifest');
      
      setLoadedTiles(prev => {
        const next = new Map(prev);
        next.set(filename, { summary, data, visible: true });
        return next;
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to load tile: ${filename}`);
    } finally {
      setLoading(false);
    }
  }, [manifest, loadedTiles]);

  const toggleVisibility = useCallback((filename: string) => {
    setLoadedTiles(prev => {
      const tile = prev.get(filename);
      if (!tile) return prev;
      
      const next = new Map(prev);
      next.set(filename, { ...tile, visible: !tile.visible });
      return next;
    });
  }, []);

  return {
    manifest,
    loadedTiles,
    loading,
    error,
    loadManifest,
    loadTile,
    toggleVisibility,
  };
}
```

**Rationale**: Custom hook encapsulates tile state and operations. Uses Map for O(1) lookup by filename. Visibility toggle allows hiding loaded tiles.

### 2. Create TileList Component

**File**: `src/debug/rmdf_viewer/ui/src/components/TileList.tsx`

**Changes**: Create sidebar component for tile selection

```tsx
import type { TileSummary } from '../types';
import type { LoadedTile } from '../hooks/useTiles';

interface TileListProps {
  tiles: TileSummary[];
  loadedTiles: Map<string, LoadedTile>;
  onLoadTile: (filename: string) => void;
  onToggleVisibility: (filename: string) => void;
  loading: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function TileList({ 
  tiles, 
  loadedTiles, 
  onLoadTile, 
  onToggleVisibility,
  loading 
}: TileListProps) {
  return (
    <div style={{ overflowY: 'auto', maxHeight: 'calc(100vh - 60px)' }}>
      <h3 style={{ margin: '0 0 1rem 0' }}>Available Tiles ({tiles.length})</h3>
      
      {loading && <p style={{ color: '#666' }}>Loading...</p>}
      
      <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
        {tiles.map(tile => {
          const loaded = loadedTiles.get(tile.filename);
          const isLoaded = !!loaded;
          
          return (
            <li 
              key={tile.filename}
              style={{ 
                padding: '0.5rem', 
                marginBottom: '0.5rem',
                background: isLoaded ? '#e8f5e9' : '#fff',
                border: '1px solid #ddd',
                borderRadius: '4px',
              }}
            >
              <div style={{ fontWeight: 'bold', marginBottom: '0.25rem' }}>
                {tile.filename}
              </div>
              
              <div style={{ fontSize: '0.85rem', color: '#666' }}>
                <div>Points: {tile.point_count.toLocaleString()}</div>
                <div>Lines: {tile.line_count.toLocaleString()}</div>
                <div>Size: {formatBytes(tile.size_bytes)}</div>
              </div>
              
              <div style={{ marginTop: '0.5rem' }}>
                {!isLoaded ? (
                  <button 
                    onClick={() => onLoadTile(tile.filename)}
                    disabled={loading}
                    style={{ padding: '0.25rem 0.5rem' }}
                  >
                    Load
                  </button>
                ) : (
                  <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                    <input
                      type="checkbox"
                      checked={loaded.visible}
                      onChange={() => onToggleVisibility(tile.filename)}
                    />
                    Visible
                  </label>
                )}
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
```

**Rationale**: 
- Shows tile metadata (point count, line count, size)
- Load button for unloaded tiles
- Visibility checkbox for loaded tiles
- Visual distinction between loaded/unloaded (green background)

### 3. Create TileBorders Component

**File**: `src/debug/rmdf_viewer/ui/src/components/TileBorders.tsx`

**Changes**: Create component to draw tile borders on map

```tsx
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
```

**Rationale**: 
- Uses Leaflet Rectangle to draw tile borders
- Dashed line style distinguishes from actual features
- Tooltip shows filename on hover
- Auto-fit bounds to show all tiles on initial load
- Cleanup removes rectangles on unmount

### 4. Update App Component

**File**: `src/debug/rmdf_viewer/ui/src/App.tsx`

**Changes**: Wire up components and state

```tsx
import { useEffect } from 'react';
import { MapContainer, TileLayer } from 'react-leaflet';
import 'leaflet/dist/leaflet.css';

import { useTiles } from './hooks/useTiles';
import { TileList } from './components/TileList';
import { TileBorders } from './components/TileBorders';

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
        </MapContainer>
      </main>
    </div>
  );
}

export default App;
```

**Rationale**: 
- Loads manifest on mount
- Passes state to child components
- Error display for failed API calls
- TileBorders renders after manifest loads

### 5. Create hooks Directory

**Directory**: Create `src/debug/rmdf_viewer/ui/src/hooks/`

Add a `.gitkeep` if needed for future hooks.

## Success Criteria

### Automated Verification:
- [ ] `bun run build` compiles without errors
- [ ] TypeScript: no type errors

### Manual Verification:
- [ ] Start backend: `ridi-router rmdf-viewer --input-dir <test-dir>`
- [ ] Start frontend dev server: `bun run dev`
- [ ] Manifest loads automatically on page load
- [ ] Tile list shows all tiles with counts and sizes
- [ ] Tile borders appear on map as dashed rectangles
- [ ] Map auto-fits to show all tile borders
- [ ] Click "Load" on a tile → button changes to "Visible" checkbox
- [ ] Loaded tile background changes to green
- [ ] Toggle visibility checkbox → no visible change yet (Phase 5 will show elements)
- [ ] Hover over tile border → tooltip shows filename
- [ ] Error shows if manifest fetch fails

## Dependencies

- Depends on: Phase 3 (Frontend Scaffolding)
- Blocks: Phase 5 (Frontend Map Visualization)

## Risks & Mitigations

- **Risk**: CORS errors when fetching API from dev server
  - **Mitigation**: Configure Vite proxy in vite.config.ts:
    ```typescript
    server: {
      proxy: {
        '/api': 'http://127.0.0.1:1337'
      }
    }
    ```

- **Risk**: Large manifests may slow down tile list rendering
  - **Mitigation**: Acceptable for debug tool; can add virtualization later if needed

## Out of Scope for This Phase

**CRITICAL**: The following items are explicitly NOT part of this phase:
- Rendering points on map - Phase 5
- Rendering lines on map - Phase 5
- Color coding by flags - Phase 5
- Popups - Phase 6
- Legend - Phase 6
- One-way direction arrows - Phase 5
- Highlighting on hover - Phase 6

## Phase Boundary Rules

**IMPORTANT**: When executing this phase:
1. **Focus on tile management only** - Loading and visibility state
2. **Borders are just rectangles** - No interaction beyond tooltips
3. **Visibility toggle works** - But nothing visible to hide yet

## Notes

- The Vite proxy configuration is essential for development. In production (Phase 7), the backend serves both API and static files.
- `useTiles` hook returns a Map for loaded tiles - this provides O(1) lookup and maintains insertion order.
- Tile borders use dashed lines to distinguish them from actual route features.
