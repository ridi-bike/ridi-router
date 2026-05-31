import { useEffect, useMemo, type PropsWithChildren } from 'react';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { registerRmdfBackgroundTask } from './backgroundTask';
import { installRmdfNotificationHandlers } from './downloads';
import { formatBytes } from './tiles';
import { useLocalValue, useRmdfDownloadingTileIds, useRmdfQueuedTileIds } from '@/tinybase/hooks';

export function RmdfDownloadProvider({ children }: PropsWithChildren) {
  useEffect(() => {
    void registerRmdfBackgroundTask();
    const notificationSubscription = installRmdfNotificationHandlers();
    return () => notificationSubscription.remove();
  }, []);

  return (
    <>
      {children}
      <GlobalDownloadIndicator />
    </>
  );
}

export function useRmdfDownloads() {
  const queuedIds = useRmdfQueuedTileIds();
  const downloadingIds = useRmdfDownloadingTileIds();
  const storedActive = useLocalValue('rmdf_downloads_active');
  const totalCount = Number(useLocalValue('rmdf_download_total_count') ?? 0);
  const completedCount = Number(useLocalValue('rmdf_download_completed_count') ?? 0);
  const totalBytes = Number(useLocalValue('rmdf_download_total_bytes') ?? 0);
  const completedBytes = Number(useLocalValue('rmdf_download_completed_bytes') ?? 0);
  const warning = (useLocalValue('rmdf_download_warning') as string | undefined) ?? null;
  const lastSummary = (useLocalValue('rmdf_download_last_summary') as string | undefined) ?? null;

  return useMemo(
    () => ({
      active: Boolean(storedActive) || queuedIds.size > 0 || downloadingIds.size > 0,
      queuedIds,
      downloadingIds,
      totalCount,
      completedCount,
      totalBytes,
      completedBytes,
      warning,
      lastSummary,
    }),
    [storedActive, queuedIds, downloadingIds, totalCount, completedCount, totalBytes, completedBytes, warning, lastSummary]
  );
}

function GlobalDownloadIndicator() {
  const insets = useSafeAreaInsets();
  const downloads = useRmdfDownloads();
  if (!downloads.active) return null;

  return (
    <View
      pointerEvents="none"
      className="absolute left-3 right-3 z-50 rounded-2xl border border-primary/40 bg-card/95 px-4 py-3 shadow-lg"
      style={{ top: insets.top + 8 }}
    >
      <Text className="text-foreground text-sm font-bold">Downloading map data</Text>
      <Text className="text-muted text-xs">
        {downloads.completedCount}/{downloads.totalCount} tiles · {formatBytes(downloads.completedBytes)} / {formatBytes(downloads.totalBytes)}
      </Text>
    </View>
  );
}
