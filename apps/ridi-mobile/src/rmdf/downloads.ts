import * as FileSystem from 'expo-file-system/legacy';
import * as Network from 'expo-network';
import * as Notifications from 'expo-notifications';
import { router } from 'expo-router';
import { Platform } from 'react-native';
import type { Store } from 'tinybase';

import { RMDF_BASE_URL, RMDF_DOWNLOAD_CONCURRENCY, RMDF_NOTIFICATION_CHANNEL_ID, RMDF_NOTIFICATION_IDENTIFIER, NOTIFICATIONS_DISABLED_WARNING } from './constants';
import { deleteRmdfTiles, ensureRmdfDirectory, getRmdfTempTilePath, getRmdfTilePath } from './storage';
import type { RmdfManifestTile } from './types';
import { getRmdfDownloadQueueRowId, RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '@/tinybase/ids';
import { markRmdfManifestTileDownloaded, markRmdfTileRemoteOnly, queueRmdfManifestTile } from '@/tinybase/local/rmdfTiles';

type DownloadQueueItem = {
  tile: RmdfManifestTile;
};

type QueueRowEntry = [string, ReturnType<Store['getRow']>];

function nowIso(): string {
  return new Date().toISOString();
}

function tileFromStore(store: Store, tileId: string): RmdfManifestTile {
  const row = store.getRow(RMDF_TILES_TABLE, tileId);
  return {
    filename: String(row.filename ?? tileId),
    col: 0,
    row: 0,
    bounds: { lat_min: 0, lat_max: 0, lon_min: 0, lon_max: 0 },
    neighbors: {},
    size_bytes: Number(row.size_bytes ?? 0),
    point_count: 0,
    line_count: 0,
    checksum: String(row.checksum ?? ''),
  };
}

function getSortedQueueEntries(store: Store): QueueRowEntry[] {
  return Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE)).sort(
    ([, a], [, b]) => Number(a.priority ?? 0) - Number(b.priority ?? 0) || String(a.queued_at ?? '').localeCompare(String(b.queued_at ?? ''))
  );
}

function getQueuedItems(store: Store, state: 'queued' | 'running'): DownloadQueueItem[] {
  return getSortedQueueEntries(store)
    .filter(([, row]) => row.state === state)
    .map(([, row]) => ({ tile: tileFromStore(store, String(row.tile_id)) }));
}

function getDownloadedCount(store: Store): number {
  return Object.values(store.getTable(RMDF_TILES_TABLE)).filter((row) => row.status === 'downloaded').length;
}

function getFailedCount(store: Store): number {
  return Object.values(store.getTable(RMDF_TILES_TABLE)).filter((row) => row.status === 'failed').length;
}

class RmdfDownloadManager {
  private processing = false;
  private notificationId: string | null = null;
  private hydrated = false;
  private localStore: Store | null = null;

  attachLocalStore(store: Store) {
    if (this.localStore === store) return;
    this.detachLocalStore();
    this.localStore = store;
  }

  detachLocalStore() {
    this.localStore = null;
    this.hydrated = false;
  }

  async hydrate() {
    if (this.hydrated) return;
    this.hydrated = true;
    await ensureRmdfDirectory();
    this.resetInterruptedRunningRows();
  }

  async enqueue(tiles: RmdfManifestTile[]) {
    const store = this.requireLocalStore();
    await ensureRmdfDirectory();
    const existing = new Set<string>();
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_TILES_TABLE))) {
      if (row.status === 'queued' || row.status === 'downloading' || row.status === 'downloaded') existing.add(rowId);
    }

    const newTiles = tiles.filter((tile) => !existing.has(tile.filename));
    if (newTiles.length === 0) return;

    const activeBeforeEnqueue = this.hasActiveRows(store);
    const addedBytes = newTiles.reduce((sum, tile) => sum + tile.size_bytes, 0);
    const queuedAt = nowIso();
    store.transaction(() => {
      if (!activeBeforeEnqueue) {
        store.setValue('rmdf_download_total_count', 0);
        store.setValue('rmdf_download_completed_count', 0);
        store.setValue('rmdf_download_total_bytes', 0);
        store.setValue('rmdf_download_completed_bytes', 0);
      }
      for (const tile of newTiles) {
        queueRmdfManifestTile(store, tile, store.getRowCount(RMDF_DOWNLOAD_QUEUE_TABLE), queuedAt);
      }
      store.setValue('rmdf_download_total_count', Number(store.getValue('rmdf_download_total_count') ?? 0) + newTiles.length);
      store.setValue('rmdf_download_total_bytes', Number(store.getValue('rmdf_download_total_bytes') ?? 0) + addedBytes);
      store.setValue('rmdf_downloads_active', true);
      store.delValue('rmdf_download_last_summary');
    });

    await this.prepareNotification();
    void this.processQueuedDownloads();
  }

  async removeQueuedOrDelete(filenames: string[]) {
    const store = this.requireLocalStore();
    const downloadingIds = new Set(getQueuedItems(store, 'running').map((item) => item.tile.filename));
    const downloadedIds = new Set(Object.entries(store.getTable(RMDF_TILES_TABLE)).filter(([, row]) => row.status === 'downloaded').map(([rowId]) => rowId));

    for (const filename of filenames) {
      const queueRowId = getRmdfDownloadQueueRowId(filename);
      const queueRow = store.getRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
      if (queueRow.state !== 'running') store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
      if (store.hasRow(RMDF_TILES_TABLE, filename) && queueRow.state !== 'running') {
        const nextStatus = downloadedIds.has(filename) ? 'downloaded' : 'remoteOnly';
        store.setCell(RMDF_TILES_TABLE, filename, 'status', nextStatus);
        store.setCell(RMDF_TILES_TABLE, filename, 'selected', false);
      }
    }

    const stillNeedDelete = filenames.filter((filename) => !downloadingIds.has(filename));
    const result = await deleteRmdfTiles(stillNeedDelete);
    const deletedAt = nowIso();
    store.transaction(() => {
      for (const filename of result.deleted) markRmdfTileRemoteOnly(store, filename, deletedAt);
      store.setValue('rmdf_download_last_summary', `Deleted ${result.deleted.length} map tile(s).${result.failed.length ? ` ${result.failed.length} failed.` : ''}`);
    });
    return result;
  }

  clearSummary() {
    this.localStore?.delValue('rmdf_download_last_summary');
  }

  async processQueuedDownloads() {
    if (getQueuedItems(this.requireLocalStore(), 'queued').length === 0) return;
    await this.startProcessing();
  }

  private async startProcessing() {
    const store = this.requireLocalStore();
    if (this.processing) return;
    this.processing = true;
    store.setValue('rmdf_downloads_active', true);

    try {
      const workers = Array.from({ length: RMDF_DOWNLOAD_CONCURRENCY }, () => this.worker());
      await Promise.all(workers);
    } finally {
      this.processing = false;
      const active = this.hasActiveRows(store);
      store.setValue('rmdf_downloads_active', active);
      if (!active) {
        store.setValue('rmdf_download_last_summary', `Downloaded ${Number(store.getValue('rmdf_download_completed_count') ?? getDownloadedCount(store))} map tile(s).${getFailedCount(store) ? ` ${getFailedCount(store)} failed.` : ''}`);
        await this.finishNotification();
      }
    }
  }

  private async worker() {
    const store = this.requireLocalStore();
    for (;;) {
      const item = this.takeNextQueuedItem();
      if (!item) return;
      const tile = item.tile;
      const queueRowId = getRmdfDownloadQueueRowId(tile.filename);
      const startedAt = nowIso();

      store.transaction(() => {
        store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, {
          state: 'running',
          attempts: Number(store.getCell(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, 'attempts') ?? 0) + 1,
          started_at: startedAt,
          updated_at: startedAt,
        });
        store.setCell(RMDF_TILES_TABLE, tile.filename, 'status', 'downloading');
      });
      await this.updateNotification();

      try {
        await this.downloadTile(tile);
        const completedAt = nowIso();
        store.transaction(() => {
          store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
          markRmdfManifestTileDownloaded(store, tile, completedAt);
          store.setValue('rmdf_download_completed_count', Number(store.getValue('rmdf_download_completed_count') ?? 0) + 1);
          store.setValue('rmdf_download_completed_bytes', Number(store.getValue('rmdf_download_completed_bytes') ?? 0) + tile.size_bytes);
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Tile download failed.';
        const failedAt = nowIso();
        store.transaction(() => {
          store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, {
            state: 'failed',
            updated_at: failedAt,
            error_message: message,
          });
          store.setPartialRow(RMDF_TILES_TABLE, tile.filename, {
            status: 'failed',
            error_message: message,
            last_checked_at: failedAt,
          });
        });
      } finally {
        await this.updateNotification();
      }
    }
  }

  private takeNextQueuedItem(): DownloadQueueItem | null {
    const store = this.requireLocalStore();
    const row = getSortedQueueEntries(store).filter(([, queueRow]) => queueRow.state === 'queued')[0];
    if (!row) return null;
    const [, queueRow] = row;
    return { tile: tileFromStore(store, String(queueRow.tile_id)) };
  }

  private async downloadTile(tile: RmdfManifestTile) {
    const destination = getRmdfTilePath(tile.filename);
    const temp = getRmdfTempTilePath(tile.filename);
    await FileSystem.deleteAsync(temp, { idempotent: true });
    const result = await FileSystem.downloadAsync(`${RMDF_BASE_URL}${tile.filename}`, temp);
    if (result.status < 200 || result.status >= 300) {
      throw new Error(`Tile download failed with HTTP ${result.status}.`);
    }
    const info = await FileSystem.getInfoAsync(temp);
    if (!info.exists || info.isDirectory || info.size !== tile.size_bytes) {
      await FileSystem.deleteAsync(temp, { idempotent: true });
      throw new Error('Downloaded tile size does not match manifest.');
    }
    await FileSystem.deleteAsync(destination, { idempotent: true });
    await FileSystem.moveAsync({ from: temp, to: destination });
  }

  private resetInterruptedRunningRows() {
    const store = this.localStore;
    if (!store) return;
    const updatedAt = nowIso();
    store.transaction(() => {
      for (const [rowId, row] of Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE))) {
        if (row.state === 'running') {
          store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, rowId, { state: 'queued', updated_at: updatedAt });
          if (typeof row.tile_id === 'string') store.setCell(RMDF_TILES_TABLE, row.tile_id, 'status', 'queued');
        }
      }
    });
  }

  private hasActiveRows(store: Store): boolean {
    return Object.values(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE)).some((row) => row.state === 'queued' || row.state === 'running');
  }

  private requireLocalStore(): Store {
    if (!this.localStore) throw new Error('RMDF downloads require TinyBase localStore initialization.');
    return this.localStore;
  }

  private async prepareNotification() {
    const store = this.localStore;
    try {
      if (Platform.OS === 'android') {
        await Notifications.setNotificationChannelAsync(RMDF_NOTIFICATION_CHANNEL_ID, {
          name: 'Map data downloads',
          importance: Notifications.AndroidImportance.LOW,
        });
      }
      const permission = await Notifications.requestPermissionsAsync();
      if (!permission.granted) {
        store?.setValue('rmdf_download_warning', NOTIFICATIONS_DISABLED_WARNING);
        return;
      }
      await this.updateNotification();
    } catch {
      store?.setValue('rmdf_download_warning', NOTIFICATIONS_DISABLED_WARNING);
    }
  }

  private async updateNotification() {
    const store = this.localStore;
    if (!store || store.getValue('rmdf_download_warning') === NOTIFICATIONS_DISABLED_WARNING) return;
    const completedCount = Number(store.getValue('rmdf_download_completed_count') ?? 0);
    const totalCount = Number(store.getValue('rmdf_download_total_count') ?? 0);
    const body = `${completedCount}/${totalCount} tiles downloaded`;
    try {
      if (this.notificationId) {
        await Notifications.dismissNotificationAsync(this.notificationId).catch(() => undefined);
      }
      this.notificationId = await Notifications.scheduleNotificationAsync({
        identifier: RMDF_NOTIFICATION_IDENTIFIER,
        content: {
          title: 'Downloading map data',
          body,
          data: { href: '/rmdf-download' },
        },
        trigger: Platform.OS === 'android' ? { channelId: RMDF_NOTIFICATION_CHANNEL_ID } : null,
      });
    } catch {
      store.setValue('rmdf_download_warning', NOTIFICATIONS_DISABLED_WARNING);
    }
  }

  private async finishNotification() {
    if (!this.notificationId) return;
    await Notifications.dismissNotificationAsync(this.notificationId).catch(() => undefined);
    this.notificationId = null;
  }
}

export const rmdfDownloadManager = new RmdfDownloadManager();

export async function isCellularConnection(): Promise<boolean> {
  const state = await Network.getNetworkStateAsync();
  return state.type === Network.NetworkStateType.CELLULAR;
}

export function installRmdfNotificationHandlers() {
  Notifications.setNotificationHandler({
    handleNotification: async () => ({
      shouldShowBanner: true,
      shouldShowList: true,
      shouldPlaySound: false,
      shouldSetBadge: false,
    }),
  });

  return Notifications.addNotificationResponseReceivedListener((response) => {
    const href = response.notification.request.content.data?.href;
    if (href === '/rmdf-download') {
      router.push('/rmdf-download' as never);
    }
  });
}
