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
      eventHandlers={{
        remove: onClose,
      }}
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
