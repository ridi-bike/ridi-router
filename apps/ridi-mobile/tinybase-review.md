# TinyBase Review

Focus: unnecessary plumbing, overly complex code, and simplification opportunities against `tinybase-plan.md`.

## Summary

The branch broadly follows the plan: two stores exist, persistence is centralized, startup blocks route rendering, and RMDF durable state has moved into `localStore`.

The main issue is that the implementation keeps several state systems alive at once:

1. TinyBase local store
2. `RmdfDownloadManager` snapshot state
3. React `RmdfDownloadProvider` context state
4. legacy JSON queue file plumbing
5. inert background-task plumbing

This creates extra synchronization code and makes the RMDF path harder to reason about than the plan requires.

## High-value simplifications

### 1. Remove the duplicate RMDF download snapshot layer

`src/rmdf/downloads.ts` keeps a full `RmdfDownloadSnapshot`, subscribes to TinyBase table/value listeners, clones Sets/Maps, and emits updates to `src/rmdf/RmdfDownloadProvider.tsx`.

But the snapshot is mostly derived from `localStore`, and `src/tinybase/hooks.ts` already derives the same sets from TinyBase:

- downloaded IDs
- selected IDs
- queued IDs
- downloading IDs
- failed IDs
- queue positions

This duplicates TinyBase reactivity with a second observer system.

Simpler shape:

- Keep `RmdfDownloadManager` as an imperative service only: `enqueue`, `removeQueuedOrDelete`, worker loop, notifications.
- Remove `RmdfDownloadSnapshot`, manager listeners, `cloneSnapshot`, and `syncSnapshotFromStore` as public UI state plumbing.
- Replace `RmdfDownloadProvider` state with TinyBase hooks such as `useLocalValue('rmdf_downloads_active')` and small RMDF selectors.
- Let `GlobalDownloadIndicator` read TinyBase directly.

This better matches the plan: “The global downloading-files indicator should be driven by `localStore`, not separate React-only state.”

### 2. Delete or isolate the legacy JSON queue file path

`migrateLegacyRmdfQueueToLocalStore` reads `getRmdfQueuePath()`, and `RmdfDownloadManager.hydrate()` writes `[]` back to that file every time.

After this migration, the queue source of truth is TinyBase. Keeping the old file path active adds confusion: there are still two apparent queue stores.

Simpler options:

- If no released app needs migration, delete the legacy queue migration entirely.
- If migration is needed, run it once and then remove routine writes to the legacy queue file from `hydrate()`.
- Keep the migration code clearly named as one-time compatibility code, not part of normal download startup.

### 3. Remove background task plumbing until it actually initializes TinyBase

`TaskManager.defineTask` calls `rmdfDownloadManager.hydrate()`, but the manager has no attached `localStore` in a headless background task. As written, the task does not restore the store, load persistence, or process queue rows.

That makes the background task look functional while doing almost nothing useful.

Simpler options:

- Remove `expo-background-task` / `expo-task-manager` integration for now.
- Or make the background task explicitly initialize TinyBase persistence before calling the manager.

Until then, this is misleading plumbing.

### 4. Collapse duplicated local/synced persistence setup

`src/tinybase/local/persistence.ts` and `src/tinybase/synced/persistence.ts` are almost identical. Same for the local/synced store factories.

A small shared helper would reduce files and surface area:

```ts
createSqlitePersistence({ store, databaseName, tableName, logPrefix })
```

Then local/synced modules can export only their config or typed wrapper. This keeps the planned split while avoiding duplicate mechanics.

### 5. Separate hooks from imperative RMDF store writes

`src/tinybase/hooks.ts` mixes React hooks with imperative mutation helpers:

- `upsertRmdfManifestTile`
- `setRmdfTileSelected`
- `clearRmdfTileSelection`

These are not hooks, and typing their `localStore` parameter as `ReturnType<typeof useLocalStore>` couples non-React helpers to React hook types.

Simpler shape:

- Keep `hooks.ts` for hooks only.
- Move RMDF store actions to something like `src/tinybase/local/rmdfTiles.ts` or `src/rmdf/rmdfStoreActions.ts`.
- Type them with `LocalStore` directly.

### 6. Trim unused progress/resume fields until implemented

The schema and manager carry partial-progress fields:

- `rmdf_download_current_bytes`
- queue `downloaded_bytes`
- queue `temp_path`
- tile `downloaded_bytes`

But `downloadTile()` uses `FileSystem.downloadAsync`, deletes the temp file before each attempt, and does not update byte progress. So partial resume/progress is not currently implemented.

Either implement resumable/progress downloads, or simplify now:

- keep only total/completed counts and bytes;
- remove `currentBytes` UI plumbing;
- avoid pretending queue rows support resume when they do not.

### 7. Reduce duplicated RMDF derived selectors

`useRmdfLocalTileSets()` derives sets from TinyBase. `RmdfDownloadManager.syncSnapshotFromStore()` derives many of the same sets. `app/rmdf-download.tsx` then consumes both `downloads` and TinyBase-derived sets.

Pick one derived-data path for UI. Prefer TinyBase hooks for UI and keep the manager write-only/imperative.

### 8. Reconsider redundant persisted cells

The plan includes both row IDs and cells like `tile_id` / `filename`, but in this implementation they are always the same value for RMDF tiles. `local_path` is also derivable from filename.

Because the plan explicitly lists these cells, this is not a correctness issue. But to reduce code noise:

- avoid passing all three values through every call site unless needed;
- centralize row creation in one helper;
- treat row ID as the primary tile identity in app logic.

## Concrete cleanup order

1. Make UI read download state directly from TinyBase hooks.
2. Shrink `RmdfDownloadManager` to imperative actions and worker state.
3. Remove `RmdfDownloadProvider` state/context, or reduce it to notification-handler installation plus indicator rendering.
4. Remove routine legacy queue-file writes.
5. Remove or fix background-task support.
6. Merge duplicated persistence helpers.
7. Move non-hook store mutations out of `hooks.ts`.
8. Drop unused progress/resume fields, or implement real progress/resume.

## Bottom line

The TinyBase foundation is in place, but the branch still carries old state-management patterns beside TinyBase. The biggest simplification is to let TinyBase be the only UI-observable RMDF state source and make the download manager an imperative writer, not a second store.