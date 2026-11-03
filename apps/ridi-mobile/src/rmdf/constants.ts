// Root of the published tile layout (see `ridi-router-cli publish-tiles`). Override for local
// testing, e.g. EXPO_PUBLIC_RMDF_BASE_URL=http://localhost:8787/rmdf/ with `adb reverse`.
export const RMDF_BASE_URL = withTrailingSlash(process.env.EXPO_PUBLIC_RMDF_BASE_URL || 'https://maps.ridi.bike/rmdf/');
export const RMDF_INDEX_URL = `${RMDF_BASE_URL}index.json`;
// RMDF format the bundled native router reads; the app downloads the release for this format.
export const RMDF_SUPPORTED_FORMAT_VERSION = 2;
export const RMDF_DIRECTORY_NAME = 'rmdf';
// Written by the app from the installed tiles; this is what the native router opens.
export const RMDF_MANIFEST_FILENAME = 'manifest.json';
export const RMDF_RELEASE_CACHE_DIRECTORY_NAME = 'rmdf-release';
export const RMDF_DOWNLOAD_CONCURRENCY = 3;
export const RMDF_BACKGROUND_TASK_NAME = 'ridi-rmdf-download-task';
export const RMDF_NOTIFICATION_CHANNEL_ID = 'rmdf-downloads';
export const RMDF_NOTIFICATION_IDENTIFIER = 'rmdf-downloads-active';

export const NETWORK_DISRUPTED_WARNING =
  'Network connection is disrupted. Showing cached map data; downloads are unavailable until connection is restored.';
export const NOTIFICATIONS_DISABLED_WARNING =
  'Downloads may be interrupted because notifications are disabled. Downloads will continue while the app is active.';
export const CELLULAR_WARNING = 'These map downloads may use mobile data. Continue?';

function withTrailingSlash(url: string): string {
  return url.endsWith('/') ? url : `${url}/`;
}
