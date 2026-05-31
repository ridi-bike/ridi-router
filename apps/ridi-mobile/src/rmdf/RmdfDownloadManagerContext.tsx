import { createContext, useContext, type PropsWithChildren } from 'react';

import type { RmdfDownloadManager } from './downloads';

const RmdfDownloadManagerContext = createContext<RmdfDownloadManager | null>(null);

export function RmdfDownloadManagerProvider({ children, manager }: PropsWithChildren<{ manager: RmdfDownloadManager }>) {
  return <RmdfDownloadManagerContext.Provider value={manager}>{children}</RmdfDownloadManagerContext.Provider>;
}

export function useRmdfDownloadManager(): RmdfDownloadManager {
  const manager = useContext(RmdfDownloadManagerContext);
  if (!manager) throw new Error('RMDF download manager must be used within TinyBaseProvider.');
  return manager;
}
