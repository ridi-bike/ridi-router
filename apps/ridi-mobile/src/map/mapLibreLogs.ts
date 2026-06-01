import { LogManager } from '@maplibre/maplibre-react-native';

let isConfigured = false;

function isIgnorablePmtilesDirectoryError(message: string): boolean {
  return message.includes('Failed to load tile') &&
    message.includes('source protomaps-basemap-v4') &&
    message.includes('Error parsing PMTiles directory: map::at:  key not found');
}

export function configureMapLibreLogs() {
  if (isConfigured) {
    return;
  }

  isConfigured = true;

  LogManager.onLog((event) => {
    if (event.level === 'error' && isIgnorablePmtilesDirectoryError(event.message)) {
      return true;
    }

    return false;
  });
}
