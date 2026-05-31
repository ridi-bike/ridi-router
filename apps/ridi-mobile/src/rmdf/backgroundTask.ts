// Background RMDF downloads are intentionally not registered during the TinyBase migration.
// Foreground downloads are driven by the runtime-scoped RmdfDownloadManager.
export async function registerRmdfBackgroundTask() {
  return;
}
