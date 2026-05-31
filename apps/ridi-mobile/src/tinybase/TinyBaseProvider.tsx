import { useEffect, useState, type PropsWithChildren } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { Provider as TinyBaseReactProvider } from 'tinybase/ui-react';

import { LOCAL_STORE_ID, SYNCED_STORE_ID } from './ids';
import { destroyTinyBaseRuntime, initializeTinyBaseRuntime, type TinyBaseRuntime } from './runtime';
import { RmdfDownloadManagerProvider } from '@/rmdf/RmdfDownloadManagerContext';

export function TinyBaseProvider({ children }: PropsWithChildren) {
  const [runtime, setRuntime] = useState<TinyBaseRuntime | null>(null);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    let cancelled = false;
    let initialized: TinyBaseRuntime | null = null;

    void initializeTinyBaseRuntime()
      .then((result) => {
        initialized = result;
        if (cancelled) {
          void destroyTinyBaseRuntime(result);
          return;
        }
        setRuntime(result);
      })
      .catch((cause) => {
        if (!cancelled) setError(cause instanceof Error ? cause : new Error('TinyBase initialization failed.'));
      });

    return () => {
      cancelled = true;
      if (initialized) void destroyTinyBaseRuntime(initialized);
    };
  }, []);

  if (error) {
    return (
      <View className="flex-1 items-center justify-center bg-background p-6">
        <Text className="text-foreground text-lg font-bold">Could not start local app storage</Text>
        <Text className="text-muted mt-2 text-center text-sm">{error.message}</Text>
      </View>
    );
  }

  if (!runtime) {
    return (
      <View className="flex-1 items-center justify-center bg-background">
        <ActivityIndicator />
        <Text className="text-muted mt-3 text-sm">Loading app storage…</Text>
      </View>
    );
  }

  return (
    <TinyBaseReactProvider storesById={{ [LOCAL_STORE_ID]: runtime.localStore, [SYNCED_STORE_ID]: runtime.syncedStore }}>
      <RmdfDownloadManagerProvider manager={runtime.rmdfDownloadManager}>{children}</RmdfDownloadManagerProvider>
    </TinyBaseReactProvider>
  );
}
