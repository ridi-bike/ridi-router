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
