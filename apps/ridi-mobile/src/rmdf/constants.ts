export const RMDF_BASE_URL = 'https://maps.ridi.bike/rmdf/';
export const RMDF_MANIFEST_URL = `${RMDF_BASE_URL}manifest.json`;
export const RMDF_DIRECTORY_NAME = 'rmdf';
export const RMDF_MANIFEST_FILENAME = 'manifest.json';
export const RMDF_DOWNLOAD_CONCURRENCY = 3;
export const RMDF_BACKGROUND_TASK_NAME = 'ridi-rmdf-download-task';
export const RMDF_NOTIFICATION_CHANNEL_ID = 'rmdf-downloads';
export const RMDF_NOTIFICATION_IDENTIFIER = 'rmdf-downloads-active';

export const NETWORK_DISRUPTED_WARNING =
  'Network connection is disrupted. Showing cached map data; downloads are unavailable until connection is restored.';
export const NOTIFICATIONS_DISABLED_WARNING =
  'Downloads may be interrupted because notifications are disabled. Downloads will continue while the app is active.';
export const CELLULAR_WARNING = 'These map downloads may use mobile data. Continue?';
