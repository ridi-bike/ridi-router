import { createContext, useContext, useEffect, useMemo, useState, type PropsWithChildren } from 'react';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { installRmdfNotificationHandlers, rmdfDownloadManager, type RmdfDownloadSnapshot } from './downloads';
import { formatBytes } from './tiles';

const RmdfDownloadContext = createContext<RmdfDownloadSnapshot>(rmdfDownloadManager.getSnapshot());

export function RmdfDownloadProvider({ children }: PropsWithChildren) {
  const [snapshot, setSnapshot] = useState<RmdfDownloadSnapshot>(rmdfDownloadManager.getSnapshot());

  useEffect(() => {
    const unsubscribe = rmdfDownloadManager.subscribe(setSnapshot);
    const notificationSubscription = installRmdfNotificationHandlers();
    void rmdfDownloadManager.hydrate();
    return () => {
      unsubscribe();
      notificationSubscription.remove();
    };
  }, []);

  const value = useMemo(() => snapshot, [snapshot]);

  return (
    <RmdfDownloadContext.Provider value={value}>
      {children}
      <GlobalDownloadIndicator snapshot={snapshot} />
    </RmdfDownloadContext.Provider>
  );
}

export function useRmdfDownloads() {
  return useContext(RmdfDownloadContext);
}

function GlobalDownloadIndicator({ snapshot }: { snapshot: RmdfDownloadSnapshot }) {
  const insets = useSafeAreaInsets();
  if (!snapshot.active) return null;

  return (
    <View
      pointerEvents="none"
      className="absolute left-3 right-3 z-50 rounded-2xl border border-primary/40 bg-card/95 px-4 py-3 shadow-lg"
      style={{ top: insets.top + 8 }}
    >
      <Text className="text-foreground text-sm font-bold">Downloading map data</Text>
      <Text className="text-muted text-xs">
        {snapshot.completedCount}/{snapshot.totalCount} tiles · {formatBytes(snapshot.completedBytes)} / {formatBytes(snapshot.totalBytes)}
      </Text>
    </View>
  );
}
