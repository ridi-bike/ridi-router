import { useMemo } from 'react';
import { useCell, useRow, useStore, useTable, useValue } from 'tinybase/ui-react';

import { ROUTE_PLANS_TABLE, ROUTES_TABLE, typedPlanRow, typedRouteRow } from '@/routes/persistence';
import type { RoutePlanRow, RouteRow } from '@/routes/types';
import { LOCAL_STORE_ID, RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE, SYNCED_STORE_ID } from './ids';

export function useLocalStore() {
  const store = useStore(LOCAL_STORE_ID);
  if (!store) throw new Error('TinyBase localStore must be used within TinyBaseProvider.');
  return store;
}

export function useSyncedStore() {
  const store = useStore(SYNCED_STORE_ID);
  if (!store) throw new Error('TinyBase syncedStore must be used within TinyBaseProvider.');
  return store;
}

export function useLocalCell(tableId: string, rowId: string, cellId: string) {
  return useCell(tableId, rowId, cellId, LOCAL_STORE_ID);
}

export function useSyncedCell(tableId: string, rowId: string, cellId: string) {
  return useCell(tableId, rowId, cellId, SYNCED_STORE_ID);
}

export function useLocalValue(valueId: string) {
  return useValue(valueId, LOCAL_STORE_ID);
}

export function useSyncedValue(valueId: string) {
  return useValue(valueId, SYNCED_STORE_ID);
}

export function useRmdfTile(tileId: string) {
  return useRow(RMDF_TILES_TABLE, tileId, LOCAL_STORE_ID);
}

export function useRmdfTileTable() {
  return useTable(RMDF_TILES_TABLE, LOCAL_STORE_ID);
}

export function useRmdfQueueTable() {
  return useTable(RMDF_DOWNLOAD_QUEUE_TABLE, LOCAL_STORE_ID);
}

export function useSyncedPlans(): RoutePlanRow[] {
  const table = useTable(ROUTE_PLANS_TABLE, SYNCED_STORE_ID);
  return useMemo(
    () => Object.values(table).map((row) => typedPlanRow(row)).filter((row) => !row.deleted_at).sort((a, b) => String(b.created_at).localeCompare(String(a.created_at))),
    [table]
  );
}

export function useSyncedPlan(planId: string | null): RoutePlanRow | null {
  const row = useRow(ROUTE_PLANS_TABLE, planId ?? '', SYNCED_STORE_ID);
  return useMemo(() => (planId && Object.keys(row).length ? typedPlanRow(row) : null), [planId, row]);
}

export function useSyncedRoutesForPlan(planId: string | null): RouteRow[] {
  const table = useTable(ROUTES_TABLE, SYNCED_STORE_ID);
  return useMemo(
    () => Object.values(table).map((row) => typedRouteRow(row)).filter((row) => row.plan_id === planId && !row.deleted_at).sort((a, b) => a.route_index - b.route_index),
    [planId, table]
  );
}

export function useRmdfSelectedTileIds(): Set<string> {
  const tiles = useRmdfTileTable();
  return useMemo(
    () => new Set(Object.entries(tiles).filter(([, row]) => row.selected === true).map(([rowId]) => rowId)),
    [tiles]
  );
}

export function useRmdfDownloadedTileIds(): Set<string> {
  const tiles = useRmdfTileTable();
  return useMemo(
    () => new Set(Object.entries(tiles).filter(([, row]) => row.status === 'downloaded').map(([rowId]) => rowId)),
    [tiles]
  );
}

export function useRmdfFailedTileIds(): Set<string> {
  const tiles = useRmdfTileTable();
  return useMemo(
    () => new Set(Object.entries(tiles).filter(([, row]) => row.status === 'failed').map(([rowId]) => rowId)),
    [tiles]
  );
}

export function useRmdfQueuedTileIds(): Set<string> {
  const queue = useRmdfQueueTable();
  return useMemo(() => new Set(Object.values(queue).filter((row) => row.state === 'queued').map((row) => String(row.tile_id))), [queue]);
}

export function useRmdfDownloadingTileIds(): Set<string> {
  const queue = useRmdfQueueTable();
  return useMemo(() => new Set(Object.values(queue).filter((row) => row.state === 'running').map((row) => String(row.tile_id))), [queue]);
}

export function useRmdfQueuePositions(): Map<string, number> {
  const queue = useRmdfQueueTable();
  return useMemo(() => {
    const rows = Object.values(queue)
      .filter((row) => row.state === 'queued')
      .sort((a, b) => Number(a.priority ?? 0) - Number(b.priority ?? 0) || String(a.queued_at ?? '').localeCompare(String(b.queued_at ?? '')));
    return new Map(rows.map((row, index) => [String(row.tile_id), index + 1]));
  }, [queue]);
}

export function useRmdfLocalTileSets() {
  const downloadedIds = useRmdfDownloadedTileIds();
  const selectedIds = useRmdfSelectedTileIds();
  const queuedIds = useRmdfQueuedTileIds();
  const downloadingIds = useRmdfDownloadingTileIds();
  const failedIds = useRmdfFailedTileIds();
  const queuePositions = useRmdfQueuePositions();
  return useMemo(
    () => ({ downloadedIds, selectedIds, queuedIds, downloadingIds, failedIds, queuePositions }),
    [downloadedIds, selectedIds, queuedIds, downloadingIds, failedIds, queuePositions]
  );
}
