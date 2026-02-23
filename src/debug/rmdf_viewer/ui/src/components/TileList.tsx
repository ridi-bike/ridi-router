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
