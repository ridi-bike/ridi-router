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
      eventHandlers={{
        remove: onClose,
      }}
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
