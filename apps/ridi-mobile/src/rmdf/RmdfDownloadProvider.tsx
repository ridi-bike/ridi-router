import { useEffect, useMemo, type PropsWithChildren } from 'react';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { installRmdfNotificationHandlers } from './downloads';
import { formatBytes } from './tiles';
import { useLocalValue, useRmdfDownloadingTileIds, useRmdfQueueTable, useRmdfQueuedTileIds } from '@/tinybase/hooks';

export function RmdfDownloadProvider({ children }: PropsWithChildren) {
  useEffect(() => {
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
  const queue = useRmdfQueueTable();
  const queuedIds = useRmdfQueuedTileIds();
  const downloadingIds = useRmdfDownloadingTileIds();
  const warning = (useLocalValue('rmdf_download_warning') as string | undefined) ?? null;
  const lastSummary = (useLocalValue('rmdf_download_last_summary') as string | undefined) ?? null;

  return useMemo(() => {
    const activeRows = Object.values(queue).filter((row) => row.state === 'queued' || row.state === 'running');
    const remainingBytes = activeRows.reduce((sum, row) => sum + Number(row.expected_bytes ?? 0), 0);

    return {
      active: activeRows.length > 0,
      queuedIds,
      downloadingIds,
      remainingCount: activeRows.length,
      remainingBytes,
      warning,
      lastSummary,
    };
  }, [queue, queuedIds, downloadingIds, warning, lastSummary]);
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
        {downloads.remainingCount} tiles remaining · {formatBytes(downloads.remainingBytes)}
      </Text>
    </View>
  );
}
