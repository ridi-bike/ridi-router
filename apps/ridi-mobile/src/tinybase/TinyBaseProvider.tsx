import { createContext, useContext, useEffect, useMemo, useState, type PropsWithChildren } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { Provider as TinyBaseReactProvider } from 'tinybase/ui-react';

import { LOCAL_STORE_ID, SYNCED_STORE_ID } from './ids';
import { initializeTinyBaseRuntime, destroyTinyBaseRuntime, type TinyBaseRuntime } from './runtime';
import type { LocalStore } from './local/store';
import type { SyncedStore } from './synced/store';
import { rmdfDownloadManager } from '@/rmdf/downloads';

type TinyBaseContextValue = {
  localStore: LocalStore;
  syncedStore: SyncedStore;
};

const TinyBaseContext = createContext<TinyBaseContextValue | null>(null);


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
      if (initialized) {
        void destroyTinyBaseRuntime(initialized);
      } else {
        rmdfDownloadManager.detachLocalStore();
      }
    };
  }, []);

  const contextValue = useMemo<TinyBaseContextValue | null>(() => {
    if (!runtime) return null;
    return { localStore: runtime.localStore, syncedStore: runtime.syncedStore };
  }, [runtime]);

  if (error) {
    return (
      <View className="flex-1 items-center justify-center bg-background p-6">
        <Text className="text-foreground text-lg font-bold">Could not start local app storage</Text>
        <Text className="text-muted mt-2 text-center text-sm">{error.message}</Text>
      </View>
    );
  }

  if (!contextValue) {
    return (
      <View className="flex-1 items-center justify-center bg-background">
        <ActivityIndicator />
        <Text className="text-muted mt-3 text-sm">Loading app storage…</Text>
      </View>
    );
  }

  return (
    <TinyBaseContext.Provider value={contextValue}>
      <TinyBaseReactProvider storesById={{ [LOCAL_STORE_ID]: contextValue.localStore, [SYNCED_STORE_ID]: contextValue.syncedStore }}>
        {children}
      </TinyBaseReactProvider>
    </TinyBaseContext.Provider>
  );
}

export function useTinyBaseContext(): TinyBaseContextValue {
  const value = useContext(TinyBaseContext);
  if (!value) throw new Error('TinyBase hooks must be used within TinyBaseProvider.');
  return value;
}
