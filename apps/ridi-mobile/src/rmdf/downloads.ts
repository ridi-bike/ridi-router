import * as BackgroundTask from 'expo-background-task';
import * as FileSystem from 'expo-file-system/legacy';
import * as Network from 'expo-network';
import * as Notifications from 'expo-notifications';
import * as TaskManager from 'expo-task-manager';
import { router } from 'expo-router';
import { Platform } from 'react-native';

import {
  RMDF_BACKGROUND_TASK_NAME,
  RMDF_BASE_URL,
  RMDF_DOWNLOAD_CONCURRENCY,
  RMDF_NOTIFICATION_CHANNEL_ID,
  RMDF_NOTIFICATION_IDENTIFIER,
  NOTIFICATIONS_DISABLED_WARNING,
} from './constants';
import {
  deleteRmdfTiles,
  ensureRmdfDirectory,
  getRmdfQueuePath,
  getRmdfTempTilePath,
  getRmdfTilePath,
  readTextIfExists,
  writeText,
} from './storage';
import type { RmdfManifestTile } from './types';

type DownloadQueueItem = {
  tile: RmdfManifestTile;
};

export type RmdfDownloadSnapshot = {
  active: boolean;
  queued: DownloadQueueItem[];
  downloading: DownloadQueueItem[];
  completedIds: Set<string>;
  failedIds: Set<string>;
  queuedIds: Set<string>;
  downloadingIds: Set<string>;
  queuePositions: Map<string, number>;
  totalCount: number;
  completedCount: number;
  failedCount: number;
  totalBytes: number;
  completedBytes: number;
  currentBytes: number;
  warning: string | null;
  lastSummary: string | null;
};

type DownloadListener = (snapshot: RmdfDownloadSnapshot) => void;

const initialSnapshot = (): RmdfDownloadSnapshot => ({
  active: false,
  queued: [],
  downloading: [],
  completedIds: new Set(),
  failedIds: new Set(),
  queuedIds: new Set(),
  downloadingIds: new Set(),
  queuePositions: new Map(),
  totalCount: 0,
  completedCount: 0,
  failedCount: 0,
  totalBytes: 0,
  completedBytes: 0,
  currentBytes: 0,
  warning: null,
  lastSummary: null,
});

function cloneSnapshot(snapshot: RmdfDownloadSnapshot): RmdfDownloadSnapshot {
  return {
    ...snapshot,
    queued: [...snapshot.queued],
    downloading: [...snapshot.downloading],
    completedIds: new Set(snapshot.completedIds),
    failedIds: new Set(snapshot.failedIds),
    queuedIds: new Set(snapshot.queuedIds),
    downloadingIds: new Set(snapshot.downloadingIds),
    queuePositions: new Map(snapshot.queuePositions),
  };
}

class RmdfDownloadManager {
  private snapshot = initialSnapshot();
  private listeners = new Set<DownloadListener>();
  private processing = false;
  private notificationId: string | null = null;
  private hydrated = false;

  getSnapshot() {
    return cloneSnapshot(this.snapshot);
  }

  subscribe(listener: DownloadListener) {
    this.listeners.add(listener);
    listener(this.getSnapshot());
    return () => this.listeners.delete(listener);
  }

  async hydrate() {
    if (this.hydrated) return;
    this.hydrated = true;
    await ensureRmdfDirectory();
    const text = await readTextIfExists(getRmdfQueuePath());
    if (!text) return;
    try {
      const parsed = JSON.parse(text) as DownloadQueueItem[];
      const queued = Array.isArray(parsed) ? parsed.filter((item) => item?.tile?.filename) : [];
      if (queued.length > 0) {
        this.snapshot.queued = queued;
        this.snapshot.totalCount = queued.length;
        this.snapshot.totalBytes = queued.reduce((sum, item) => sum + item.tile.size_bytes, 0);
        this.snapshot.active = true;
        this.recomputeDerived();
        this.emit();
        void this.startProcessing();
      }
    } catch {
      await writeText(getRmdfQueuePath(), '[]');
    }
  }

  async enqueue(tiles: RmdfManifestTile[]) {
    await ensureRmdfDirectory();
    const existing = new Set([
      ...this.snapshot.queued.map((item) => item.tile.filename),
      ...this.snapshot.downloading.map((item) => item.tile.filename),
    ]);
    const newItems = tiles
      .filter((tile) => !existing.has(tile.filename))
      .map((tile) => ({ tile }));
    if (newItems.length === 0) return;

    this.snapshot.queued.push(...newItems);
    this.snapshot.totalCount += newItems.length;
    this.snapshot.totalBytes += newItems.reduce((sum, item) => sum + item.tile.size_bytes, 0);
    this.snapshot.active = true;
    this.snapshot.lastSummary = null;
    this.recomputeDerived();
    await this.persistQueue();
    await this.prepareNotification();
    await this.registerBackgroundTask();
    this.emit();
    void this.startProcessing();
  }

  async removeQueuedOrDelete(filenames: string[]) {
    const removeSet = new Set(filenames);
    const before = this.snapshot.queued.length;
    this.snapshot.queued = this.snapshot.queued.filter((item) => !removeSet.has(item.tile.filename));
    const removedQueued = before - this.snapshot.queued.length;
    if (removedQueued > 0) {
      this.snapshot.totalCount = Math.max(0, this.snapshot.totalCount - removedQueued);
      this.snapshot.totalBytes = this.snapshot.queued.reduce((sum, item) => sum + item.tile.size_bytes, 0) +
        this.snapshot.downloading.reduce((sum, item) => sum + item.tile.size_bytes, 0) +
        this.snapshot.completedBytes;
      await this.persistQueue();
    }
    const stillNeedDelete = filenames.filter((filename) => !this.snapshot.downloadingIds.has(filename));
    const result = await deleteRmdfTiles(stillNeedDelete);
    for (const filename of result.deleted) {
      this.snapshot.completedIds.delete(filename);
    }
    this.snapshot.lastSummary = `Deleted ${result.deleted.length} map tile(s).${result.failed.length ? ` ${result.failed.length} failed.` : ''}`;
    this.recomputeDerived();
    this.emit();
    return result;
  }

  clearSummary() {
    this.snapshot.lastSummary = null;
    this.emit();
  }

  private async startProcessing() {
    if (this.processing) return;
    this.processing = true;
    this.snapshot.active = true;
    this.recomputeDerived();
    this.emit();

    try {
      const workers = Array.from({ length: RMDF_DOWNLOAD_CONCURRENCY }, () => this.worker());
      await Promise.all(workers);
    } finally {
      this.processing = false;
      this.snapshot.active = this.snapshot.queued.length > 0 || this.snapshot.downloading.length > 0;
      if (!this.snapshot.active) {
        this.snapshot.lastSummary = `Downloaded ${this.snapshot.completedCount} map tile(s).${this.snapshot.failedCount ? ` ${this.snapshot.failedCount} failed.` : ''}`;
        await this.finishNotification();
      }
      await this.persistQueue();
      this.recomputeDerived();
      this.emit();
    }
  }

  private async worker() {
    for (;;) {
      const item = this.snapshot.queued.shift();
      if (!item) return;
      this.snapshot.downloading.push(item);
      this.snapshot.currentBytes = 0;
      this.recomputeDerived();
      await this.persistQueue();
      await this.updateNotification();
      this.emit();

      try {
        await this.downloadTile(item.tile);
        this.snapshot.completedIds.add(item.tile.filename);
        this.snapshot.completedCount += 1;
        this.snapshot.completedBytes += item.tile.size_bytes;
      } catch {
        this.snapshot.failedIds.add(item.tile.filename);
        this.snapshot.failedCount += 1;
      } finally {
        this.snapshot.downloading = this.snapshot.downloading.filter((downloading) => downloading.tile.filename !== item.tile.filename);
        this.snapshot.currentBytes = 0;
        this.recomputeDerived();
        await this.persistQueue();
        await this.updateNotification();
        this.emit();
      }
    }
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

  private recomputeDerived() {
    this.snapshot.queuedIds = new Set(this.snapshot.queued.map((item) => item.tile.filename));
    this.snapshot.downloadingIds = new Set(this.snapshot.downloading.map((item) => item.tile.filename));
    this.snapshot.queuePositions = new Map(this.snapshot.queued.map((item, index) => [item.tile.filename, index + 1]));
  }

  private emit() {
    const snapshot = this.getSnapshot();
    for (const listener of this.listeners) listener(snapshot);
  }

  private async persistQueue() {
    await writeText(getRmdfQueuePath(), JSON.stringify(this.snapshot.queued));
  }

  private async registerBackgroundTask() {
    try {
      const status = await BackgroundTask.getStatusAsync();
      if (status !== BackgroundTask.BackgroundTaskStatus.Available) return;
      const registered = await TaskManager.isTaskRegisteredAsync(RMDF_BACKGROUND_TASK_NAME);
      if (!registered) {
        await BackgroundTask.registerTaskAsync(RMDF_BACKGROUND_TASK_NAME, { minimumInterval: 15 });
      }
    } catch {
      // Background execution support varies by platform and build type. Foreground downloads still continue.
    }
  }

  private async prepareNotification() {
    try {
      if (Platform.OS === 'android') {
        await Notifications.setNotificationChannelAsync(RMDF_NOTIFICATION_CHANNEL_ID, {
          name: 'Map data downloads',
          importance: Notifications.AndroidImportance.LOW,
        });
      }
      const permission = await Notifications.requestPermissionsAsync();
      if (!permission.granted) {
        this.snapshot.warning = NOTIFICATIONS_DISABLED_WARNING;
        return;
      }
      await this.updateNotification();
    } catch {
      this.snapshot.warning = NOTIFICATIONS_DISABLED_WARNING;
    }
  }

  private async updateNotification() {
    if (this.snapshot.warning === NOTIFICATIONS_DISABLED_WARNING) return;
    const body = `${this.snapshot.completedCount}/${this.snapshot.totalCount} tiles downloaded`;
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
      this.snapshot.warning = NOTIFICATIONS_DISABLED_WARNING;
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

if (!TaskManager.isTaskDefined(RMDF_BACKGROUND_TASK_NAME)) {
  TaskManager.defineTask(RMDF_BACKGROUND_TASK_NAME, async () => {
    try {
      await rmdfDownloadManager.hydrate();
      return BackgroundTask.BackgroundTaskResult.Success;
    } catch {
      return BackgroundTask.BackgroundTaskResult.Failed;
    }
  });
}
