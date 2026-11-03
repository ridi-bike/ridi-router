import * as FileSystem from 'expo-file-system/legacy';
import * as Network from 'expo-network';
import * as Notifications from 'expo-notifications';
import { router } from 'expo-router';
import { Platform } from 'react-native';
import type { Store } from 'tinybase';

import { RMDF_DOWNLOAD_CONCURRENCY, RMDF_NOTIFICATION_CHANNEL_ID, RMDF_NOTIFICATION_IDENTIFIER, NOTIFICATIONS_DISABLED_WARNING } from './constants';
import { scheduleInstalledManifestWrite, writeInstalledManifest } from './installed';
import { getRmdfTileUrl } from './manifest';
import { deleteRmdfTiles, ensureRmdfDirectory, getRmdfTempTilePath, getRmdfTilePath } from './storage';
import type { RmdfManifestTile } from './types';
import { getRmdfDownloadQueueRowId, RMDF_DOWNLOAD_QUEUE_TABLE, RMDF_TILES_TABLE } from '@/tinybase/ids';
import { markRmdfManifestTileDownloaded, markRmdfTileRemoteOnly, queueRmdfManifestTile, rowTileMeta } from '@/tinybase/local/rmdfTiles';

type DownloadTile = RmdfManifestTile;

type DownloadQueueItem = {
  tile: DownloadTile;
};

type QueueRowEntry = [string, ReturnType<Store['getRow']>];

function nowIso(): string {
  return new Date().toISOString();
}

function downloadTileFromRows(store: Store, queueRow: ReturnType<Store['getRow']>): DownloadTile | null {
  const filename = String(queueRow.filename ?? queueRow.tile_id ?? '');
  return rowTileMeta(store.getRow(RMDF_TILES_TABLE, filename));
}

function getSortedQueueEntries(store: Store): QueueRowEntry[] {
  return Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE)).sort(
    ([, a], [, b]) => Number(a.priority ?? 0) - Number(b.priority ?? 0) || String(a.queued_at ?? '').localeCompare(String(b.queued_at ?? ''))
  );
}

function getQueuedItems(store: Store, state: 'queued' | 'running'): DownloadQueueItem[] {
  return getSortedQueueEntries(store)
    .filter(([, row]) => row.state === state)
    .flatMap(([, row]) => {
      const tile = downloadTileFromRows(store, row);
      return tile ? [{ tile }] : [];
    });
}

function getFailedCount(store: Store): number {
  return Object.values(store.getTable(RMDF_TILES_TABLE)).filter((row) => row.status === 'failed').length;
}

function getActiveQueueRows(store: Store) {
  return Object.values(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE)).filter((row) => row.state === 'queued' || row.state === 'running');
}

export class RmdfDownloadManager {
  private processing = false;
  private notificationId: string | null = null;
  private hydrated = false;

  constructor(private readonly localStore: Store) {}

  async hydrate() {
    if (this.hydrated) return;
    this.hydrated = true;
    await ensureRmdfDirectory();
    await this.resetInterruptedRunningRows();
  }

  async enqueue(tiles: RmdfManifestTile[]) {
    const store = this.localStore;
    await ensureRmdfDirectory();
    // Skip tiles already on their way, and installed tiles that are current. Installed tiles with
    // a different checksum are outdated and get downloaded again.
    const newTiles = tiles.filter((tile) => {
      const row = store.getRow(RMDF_TILES_TABLE, tile.filename);
      if (row.status === 'queued' || row.status === 'downloading') return false;
      return row.installed_checksum !== tile.checksum;
    });
    if (newTiles.length === 0) return;

    const queuedAt = nowIso();
    store.transaction(() => {
      for (const tile of newTiles) {
        queueRmdfManifestTile(store, tile, store.getRowCount(RMDF_DOWNLOAD_QUEUE_TABLE), queuedAt);
      }
      store.delValue('rmdf_download_last_summary');
    });

    await this.prepareNotification();
    void this.processQueuedDownloads();
  }

  async removeQueuedOrDelete(filenames: string[]) {
    const store = this.localStore;
    const downloadingIds = new Set(getQueuedItems(store, 'running').map((item) => item.tile.filename));
    const downloadedIds = new Set(Object.entries(store.getTable(RMDF_TILES_TABLE)).filter(([, row]) => row.status === 'downloaded').map(([rowId]) => rowId));

    for (const filename of filenames) {
      const queueRowId = getRmdfDownloadQueueRowId(filename);
      const queueRow = store.getRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
      if (queueRow.state !== 'running') store.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId);
      if (store.hasRow(RMDF_TILES_TABLE, filename) && queueRow.state !== 'running') {
        const installed = downloadedIds.has(filename) || !!store.getCell(RMDF_TILES_TABLE, filename, 'installed_meta');
        const nextStatus = installed ? 'downloaded' : 'remoteOnly';
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
    await writeInstalledManifest(store);
    return result;
  }

  clearSummary() {
    this.localStore.delValue('rmdf_download_last_summary');
  }

  async processQueuedDownloads() {
    if (getQueuedItems(this.localStore, 'queued').length === 0) return;
    await this.startProcessing();
  }

  private async startProcessing() {
    const store = this.localStore;
    if (this.processing) return;
    this.processing = true;

    try {
      const workers = Array.from({ length: RMDF_DOWNLOAD_CONCURRENCY }, () => this.worker());
      await Promise.all(workers);
    } finally {
      this.processing = false;
      await writeInstalledManifest(store);
      if (!this.hasActiveRows(store)) {
        store.setValue('rmdf_download_last_summary', `Downloaded queued map tile(s).${getFailedCount(store) ? ` ${getFailedCount(store)} failed.` : ''}`);
        await this.finishNotification();
      }
    }
  }

  private async worker() {
    const store = this.localStore;
    for (;;) {
      const item = this.takeNextQueuedItem();
      if (!item) return;
      const tile = item.tile;
      if (!tile.filename) continue;
      const queueRowId = getRmdfDownloadQueueRowId(tile.filename);
      const startedAt = nowIso();
      const tempPath = getRmdfTempTilePath(tile.filename);

      store.transaction(() => {
        store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, {
          state: 'running',
          attempts: Number(store.getCell(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, 'attempts') ?? 0) + 1,
          started_at: startedAt,
          updated_at: startedAt,
          temp_path: tempPath,
          downloaded_bytes: 0,
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
        });
        scheduleInstalledManifestWrite(store);
      } catch (error) {
        const message = error instanceof Error ? error.message : 'Tile download failed.';
        const failedAt = nowIso();
        const tempInfo = await FileSystem.getInfoAsync(tempPath).catch(() => null);
        store.transaction(() => {
          store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, queueRowId, {
            state: 'failed',
            updated_at: failedAt,
            error_message: message,
            temp_path: tempPath,
            downloaded_bytes: tempInfo?.exists && !tempInfo.isDirectory ? tempInfo.size : 0,
          });
          // A failed update leaves the previously installed tile in place and usable.
          const installed = !!store.getCell(RMDF_TILES_TABLE, tile.filename, 'installed_meta');
          store.setPartialRow(RMDF_TILES_TABLE, tile.filename, {
            status: installed ? 'downloaded' : 'failed',
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
    for (const [rowId, queueRow] of getSortedQueueEntries(this.localStore)) {
      if (queueRow.state !== 'queued') continue;
      const tile = downloadTileFromRows(this.localStore, queueRow);
      if (tile) return { tile };
      // Queued before tiles carried release metadata; it cannot be downloaded.
      this.localStore.delRow(RMDF_DOWNLOAD_QUEUE_TABLE, rowId);
    }
    return null;
  }

  private async downloadTile(tile: DownloadTile) {
    const destination = getRmdfTilePath(tile.filename);
    const temp = getRmdfTempTilePath(tile.filename);
    const tempInfo = await FileSystem.getInfoAsync(temp).catch(() => null);
    if (tempInfo?.exists && !tempInfo.isDirectory && tempInfo.size === tile.size_bytes) {
      await FileSystem.deleteAsync(destination, { idempotent: true });
      await FileSystem.moveAsync({ from: temp, to: destination });
      return;
    }
    const result = await FileSystem.downloadAsync(getRmdfTileUrl(tile.path), temp);
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

  private async resetInterruptedRunningRows() {
    const store = this.localStore;
    const updatedAt = nowIso();
    const updates: { rowId: string; tileId?: string; downloadedBytes: number; tempPath?: string }[] = [];
    for (const [rowId, row] of Object.entries(store.getTable(RMDF_DOWNLOAD_QUEUE_TABLE))) {
      if (row.state !== 'running') continue;
      const tileId = typeof row.tile_id === 'string' ? row.tile_id : undefined;
      const tempPath = typeof row.temp_path === 'string' ? row.temp_path : tileId ? getRmdfTempTilePath(tileId) : undefined;
      const tempInfo = tempPath ? await FileSystem.getInfoAsync(tempPath).catch(() => null) : null;
      updates.push({
        rowId,
        tileId,
        tempPath,
        downloadedBytes: tempInfo?.exists && !tempInfo.isDirectory ? tempInfo.size : 0,
      });
    }
    store.transaction(() => {
      for (const update of updates) {
        store.setPartialRow(RMDF_DOWNLOAD_QUEUE_TABLE, update.rowId, {
          state: 'queued',
          updated_at: updatedAt,
          downloaded_bytes: update.downloadedBytes,
          ...(update.tempPath ? { temp_path: update.tempPath } : null),
        });
        if (update.tileId) store.setCell(RMDF_TILES_TABLE, update.tileId, 'status', 'queued');
      }
    });
  }

  private hasActiveRows(store: Store): boolean {
    return getActiveQueueRows(store).length > 0;
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
        store.setValue('rmdf_download_warning', NOTIFICATIONS_DISABLED_WARNING);
        return;
      }
      await this.updateNotification();
    } catch {
      store.setValue('rmdf_download_warning', NOTIFICATIONS_DISABLED_WARNING);
    }
  }

  private async updateNotification() {
    const store = this.localStore;
    if (store.getValue('rmdf_download_warning') === NOTIFICATIONS_DISABLED_WARNING) return;
    const rows = getActiveQueueRows(store);
    const runningCount = rows.filter((row) => row.state === 'running').length;
    const body = `${runningCount}/${rows.length} tiles downloading`;
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
