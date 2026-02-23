# Phase 3: Frontend Scaffolding

## Overview

Initialize the Vite React project, install dependencies, and create the basic application structure with an empty Leaflet map. This phase establishes the frontend foundation that subsequent phases will build upon.

## Changes Required

### 1. Initialize Vite Project

**Command**: Run from project root

```bash
bun create vite src/debug/rmdf_viewer/ui --template react-ts
```

**Result**: Creates `src/debug/rmdf_viewer/ui/` with:
- `package.json`
- `vite.config.ts`
- `tsconfig.json`
- `index.html`
- `src/main.tsx`
- `src/App.tsx`

**Rationale**: Using Vite with React TypeScript template provides fast development, TypeScript support, and optimized builds. The `bun` runtime is specified per user requirement.

### 2. Install Dependencies

**File**: `src/debug/rmdf_viewer/ui/package.json`

**Changes**: Add Leaflet and React-Leaflet dependencies

```bash
cd src/debug/rmdf_viewer/ui
bun add leaflet react-leaflet
bun add -D @types/leaflet
```

**Result**: package.json dependencies should include:

```json
{
  "dependencies": {
    "react": "^18.x",
    "react-dom": "^18.x",
    "leaflet": "^1.9.x",
    "react-leaflet": "^4.x"
  },
  "devDependencies": {
    "@types/leaflet": "^1.9.x",
    "@vitejs/plugin-react": "^4.x",
    "typescript": "^5.x",
    "vite": "^6.x"
  }
}
```

**Rationale**: 
- `leaflet`: Core mapping library
- `react-leaflet`: React bindings for Leaflet
- `@types/leaflet`: TypeScript definitions

### 3. Configure Vite Build Output

**File**: `src/debug/rmdf_viewer/ui/vite.config.ts`

**Changes**: Set build output to `dist` and configure for embedding

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    minify: 'esbuild',
  },
  base: '/',  // Relative paths for embedding
})
```

**Rationale**: Clean output directory, no sourcemaps (not needed for debug tool), ESBuild minification for fast builds.

### 4. Generate TypeScript Types from Rust

The TypeScript types are automatically generated from the Rust API structs using `ts-rs`. This ensures type safety between backend and frontend.

**Command**: Generate types by building the backend with the feature enabled

```bash
# Generate TypeScript types from Rust structs
cargo build --features rmdf-viewer
```

**Result**: Creates `src/debug/rmdf_viewer/ui/src/types/generated/` with:
- `ManifestResponse.ts`
- `TileSummary.ts`
- `TileBoundsResponse.ts`
- `TileResponse.ts`
- `TileHeader.ts`
- `PointResponse.ts`
- `LineResponse.ts`
- `TagResponse.ts`

**File**: `src/debug/rmdf_viewer/ui/src/types/index.ts`

**Changes**: Create barrel file to re-export generated types

```typescript
// Re-export all types from ts-rs generated files
export type {
  ManifestResponse,
  TileSummary,
  TileBoundsResponse,
} from './generated/ManifestResponse';

export type {
  TileResponse,
  TileHeader,
  PointResponse,
  LineResponse,
  TagResponse,
} from './generated/TileResponse';
```

**Rationale**: Using `ts-rs` ensures TypeScript types are always in sync with Rust structs. The types are generated during the Cargo build process, eliminating manual type maintenance and preventing type mismatches.

### 5. Create API Client

**File**: `src/debug/rmdf_viewer/ui/src/api/client.ts`

**Changes**: Implement API client functions

```typescript
import type { ManifestResponse, TileResponse } from '../types';

const API_BASE = '/api';

export async function fetchManifest(): Promise<ManifestResponse> {
  const response = await fetch(`${API_BASE}/manifest`);
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error || 'Failed to fetch manifest');
  }
  return response.json();
}

export async function fetchTile(filename: string): Promise<TileResponse> {
  const response = await fetch(`${API_BASE}/tiles/${filename}`);
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error || `Failed to fetch tile: ${filename}`);
  }
  return response.json();
}
```

**Rationale**: Simple fetch-based client with error handling. No external HTTP library needed for this use case.

### 6. Create Basic App Structure

**File**: `src/debug/rmdf_viewer/ui/src/App.tsx`

**Changes**: Replace default with basic layout

```tsx
import { useState } from 'react';
import { MapContainer, TileLayer } from 'react-leaflet';
import 'leaflet/dist/leaflet.css';

function App() {
  return (
    <div style={{ display: 'flex', height: '100vh' }}>
      {/* Sidebar - Phase 4 */}
      <aside style={{ width: '300px', padding: '1rem', background: '#f5f5f5' }}>
        <h2>RMDF Debug Viewer</h2>
        <p>Tile list will appear here</p>
      </aside>

      {/* Map */}
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
        </MapContainer>
      </main>
    </div>
  );
}

export default App;
```

**Rationale**: Basic two-column layout with sidebar for tile list (Phase 4) and main area for map. Initial zoom level 2 shows full world.

### 7. Update index.html

**File**: `src/debug/rmdf_viewer/ui/index.html`

**Changes**: Update title

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>RMDF Debug Viewer</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

**Rationale**: Descriptive title for the debug viewer.

### 8. Create Directory Structure

**Directory structure after this phase**:

```
src/debug/rmdf_viewer/ui/
├── index.html
├── package.json
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/
│   │   └── client.ts
│   ├── types/
│   │   ├── manifest.ts
│   │   └── tile.ts
│   └── components/       # Empty, created for Phase 4
│       └── .gitkeep
└── dist/                 # Created by build (Phase 7)
```

## Success Criteria

### Automated Verification:
- [ ] `bun install` completes without errors in ui/ directory
- [ ] `bun run build` creates dist/ directory with index.html
- [ ] `bun run dev` starts development server
- [ ] TypeScript compiles without errors: `bun run build`

### Manual Verification:
- [ ] Open development server URL in browser
- [ ] Page loads without console errors
- [ ] Leaflet map appears with OpenStreetMap tiles
- [ ] Sidebar shows placeholder text "Tile list will appear here"
- [ ] Map is interactive (can pan/zoom)

## Dependencies

- Depends on: Phase 2 (Backend API) - for API types contract
- Blocks: Phase 4 (Frontend Tile Management)

## Risks & Mitigations

- **Risk**: Leaflet CSS not loading correctly
  - **Mitigation**: Ensure `leaflet.css` import is first in App.tsx

- **Risk**: TypeScript version mismatch
  - **Mitigation**: Use Vite template defaults, don't override versions

## Out of Scope for This Phase

**CRITICAL**: The following items are explicitly NOT part of this phase:
- Tile list component - Phase 4
- Map tile borders - Phase 4
- Point/line rendering - Phase 5
- Popups - Phase 6
- Legend - Phase 6
- Loading actual data from API - Phase 4
- Build output embedding - Phase 7

## Phase Boundary Rules

**IMPORTANT**: When executing this phase:
1. **Focus on scaffolding only** - Empty map, no data loading
2. **Use bun commands** - `bun create vite`, `bun add`, not npm/yarn
3. **Don't connect to backend yet** - Just verify dev server works standalone

## Notes

- Leaflet uses a global `L` object. With react-leaflet, we use React components instead.
- The map center at [0, 0] with zoom 2 will be adjusted when tiles are loaded (Phase 4).
- For development, `bun run dev` runs the Vite dev server on a different port than the backend. The backend serves the built UI (Phase 7). During development, we can proxy API requests or run both servers.
