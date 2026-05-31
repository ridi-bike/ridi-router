import * as BackgroundTask from 'expo-background-task';
import * as TaskManager from 'expo-task-manager';

import { RMDF_BACKGROUND_TASK_NAME } from './constants';
import { rmdfDownloadManager } from './downloads';
import { destroyTinyBaseRuntime, initializeTinyBaseRuntime } from '@/tinybase/runtime';

export async function registerRmdfBackgroundTask() {
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

if (!TaskManager.isTaskDefined(RMDF_BACKGROUND_TASK_NAME)) {
  TaskManager.defineTask(RMDF_BACKGROUND_TASK_NAME, async () => {
    let runtime: Awaited<ReturnType<typeof initializeTinyBaseRuntime>> | null = null;
    try {
      runtime = await initializeTinyBaseRuntime({ startQueuedDownloads: false });
      await rmdfDownloadManager.processQueuedDownloads();
      return BackgroundTask.BackgroundTaskResult.Success;
    } catch {
      return BackgroundTask.BackgroundTaskResult.Failed;
    } finally {
      if (runtime) await destroyTinyBaseRuntime(runtime);
    }
  });
}
