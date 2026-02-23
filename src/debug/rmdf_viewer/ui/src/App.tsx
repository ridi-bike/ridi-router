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
