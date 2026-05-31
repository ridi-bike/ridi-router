# TinyBase Branch Review

Scope: review current TinyBase/RMDF implementation against `tinybase-plan.md`, with emphasis on unnecessary plumbing, complexity, and simplification opportunities.

## Overall Assessment

The branch broadly follows the plan: two stores exist, app rendering is blocked until TinyBase initializes, persistence is centralized, RMDF durable state moved into `localStore`, and routes do not create stores or persisters.

The main issue is not missing architecture. The main issue is excess coordination code around that architecture: duplicate provider state, thin wrapper modules, a global download manager that has to be manually attached to TinyBase, and persisted aggregate values that duplicate queue/tile state.

## Highest-Value Simplifications

### 1. Remove one of the two TinyBase provider layers

`TinyBaseProvider` currently maintains a custom React context and also mounts TinyBase's `Provider` with `storesById`.

Relevant code:

- `src/tinybase/TinyBaseProvider.tsx`
- `src/tinybase/hooks.ts`

Why this feels over-plumbed:

- `useLocalStore()` and `useSyncedStore()` read from the custom context.
- `useLocalCell()`, `useLocalValue()`, `useRmdfTileTable()`, etc. read through TinyBase's own provider by store ID.
- This creates two store access paths that must stay consistent.

Simpler options:

1. Prefer TinyBase's provider as the only provider, and implement `useLocalStore()` / `useSyncedStore()` through TinyBase's store hook if available.
2. Or keep the custom context and pass concrete stores directly into lower-level hooks instead of using TinyBase store IDs.

Recommendation: use TinyBase's provider as the single source of truth. The custom context adds little value beyond fatal-startup gating.

### 2. Collapse pass-through store and persistence modules

Several modules are only constant-binding wrappers:

- `src/tinybase/local/store.ts`
- `src/tinybase/synced/store.ts`
- `src/tinybase/local/persistence.ts`
- `src/tinybase/synced/persistence.ts`
- `src/tinybase/store.ts`
- `src/tinybase/persistence.ts`

This matches the proposed file layout, but the implementation is small enough that the indirection makes the code harder to follow than necessary.

Simpler shape:

- Keep `local/schema.ts` and `synced/schema.ts` separate.
- Put store creation and SQLite persister creation in one runtime factory module.
- Keep constants in `ids.ts` or `config.ts`.

This would reduce the startup path from several wrapper hops to one readable sequence:

1. create stores
2. apply schemas
3. create persisters
4. load
5. reconcile
6. start autosave

The future `syncedStore` switch to `MergeableStore` can still be contained in the runtime factory without preserving all the current pass-through files.

### 3. Replace the global attach/detach download manager with explicit store ownership

`rmdfDownloadManager` is a module-level singleton with mutable lifecycle state:

- `attachLocalStore()`
- `detachLocalStore()`
- `requireLocalStore()`
- `hydrated`
- `processing`
- `notificationId`

Relevant code:

- `src/rmdf/downloads.ts`
- `src/tinybase/runtime.ts`
- `src/tinybase/TinyBaseProvider.tsx`
- `src/rmdf/backgroundTask.ts`

Why this is complex:

- TinyBase runtime owns store creation, but the download manager owns long-lived behavior and needs manual attachment.
- Provider teardown has a special branch just to detach the manager if initialization did not finish.
- Background tasks initialize a full TinyBase runtime, attach the singleton, process work, then destroy the runtime.
- The singleton can outlive the runtime it was attached to.

Simpler options:

1. Create a download manager instance inside `initializeTinyBaseRuntime()` and return it with the runtime.
2. Or make download operations plain functions that take `localStore` explicitly.
3. Or expose an `RmdfDownloadService` from the provider, scoped to the provider lifecycle.

Recommendation: avoid a global mutable singleton. The store is already initialized before routes render, so callers can access it through hooks or a scoped service.

### 4. Do not persist aggregate download UI values unless they are true durable state

The local store persists these values:

- `rmdf_downloads_active`
- `rmdf_download_total_count`
- `rmdf_download_completed_count`
- `rmdf_download_total_bytes`
- `rmdf_download_completed_bytes`
- `rmdf_download_warning`
- `rmdf_download_last_summary`

Some of these are useful UX state, but several duplicate data already present in queue rows and tile rows.

Examples:

- `rmdf_downloads_active` is already derived from queued/running rows. `useRmdfDownloads()` also ORs it with queue state, which confirms it is not authoritative.
- completed counts and bytes are manually incremented while tile rows and queue rows are also updated.
- totals are reset conditionally based on whether active rows exist, which creates batch-like behavior without an explicit batch model.

Simpler options:

1. Derive `active` from queue rows only.
2. Derive queued/running counts from `rmdf_download_queue`.
3. Keep only user-visible non-derived values, such as warning and last summary.
4. If batch progress must survive restart, introduce a small explicit `rmdf_download_batches` table instead of loose aggregate values.

Recommendation: remove `rmdf_downloads_active` first. It is the clearest duplicate. Then decide whether the total/completed fields are a real persisted batch model or just UI state.

### 5. Avoid reconstructing fake manifest tiles from TinyBase rows

`downloads.ts` reconstructs `RmdfManifestTile` values from TinyBase rows with placeholder geometry:

```ts
{
  filename,
  col: 0,
  row: 0,
  bounds: { lat_min: 0, lat_max: 0, lon_min: 0, lon_max: 0 },
  neighbors: {},
  size_bytes,
  point_count: 0,
  line_count: 0,
  checksum,
}
```

This is a sign that the download worker does not actually need a full manifest tile.

Simpler shape:

- Define a narrow `DownloadTile` or `QueuedDownload` type with only:
  - `filename`
  - `size_bytes` / `expected_bytes`
  - `checksum` if needed
- Let manifest tiles be used at the UI/manifest boundary only.
- Let queue rows drive download workers directly.

This removes invalid placeholder data and makes the queue table a clearer source of truth.

## Plan Alignment Issues That Also Affect Simplicity

### 6. Partial download restart is planned but not represented

The plan includes queue fields for partial download recovery:

- `downloaded_bytes`
- `temp_path`

The implementation does not include these fields in `src/tinybase/local/schema.ts`, and `downloadTile()` deletes the temp file before each download.

This is simpler than the plan, but it conflicts with the stated requirement that partial downloads can be restarted with enough local state to support restart.

Recommendation: choose one explicit direction:

- If restart means "retry from scratch", update the plan/schema and remove partial-download wording.
- If resumable partial downloads are required, add the fields and make the downloader use them.

Do not keep halfway plumbing for restart semantics.

### 7. Background task support may be premature

`src/rmdf/backgroundTask.ts` initializes the full TinyBase runtime, including the unused synced store, just to process local RMDF queue work.

This adds complexity:

- background task registration
- task definition at module load
- full runtime startup inside the task
- singleton download manager attach/detach through runtime initialization
- full runtime destroy afterward

If background downloading is not required for the first TinyBase migration, defer it. If it is required, consider a local-only runtime initializer for the background task so the synced store is not opened for local queue work.

## Smaller Cleanups

### 8. Legacy queue migration can be removed before release if no released app wrote it

`migrateLegacyRmdfQueueToLocalStore()` reads `download-queue.json`, imports rows, and overwrites the file with `[]`.

If this app version has not shipped with the legacy queue file, this migration is unnecessary plumbing. Remove it before release, or isolate it behind a clearly named one-time legacy migration module.

### 9. Keep RMDF-specific write helpers, but avoid making every write path a helper

`src/tinybase/local/rmdfTiles.ts` is useful because it centralizes row IDs and stable persisted cells. However, some helpers are just small wrappers around one or two TinyBase calls.

Keep helpers that preserve invariants:

- upsert manifest tile
- queue manifest tile
- mark downloaded
- mark remote-only

Avoid adding more wrappers for simple one-off writes unless they protect a real invariant.

## Suggested Simplification Order

1. Remove or derive `rmdf_downloads_active`.
2. Replace fake `RmdfManifestTile` reconstruction with a narrow queue/download type.
3. Remove one provider access path: custom context or TinyBase provider store lookups.
4. Scope the download manager to the TinyBase runtime/provider instead of using a global attach/detach singleton.
5. Collapse pass-through TinyBase store/persistence modules if the code remains small.
6. Decide whether partial-download restart is required; then either implement it fully or remove the unused schema/planning surface.
7. Defer or localize background-task runtime startup.

## Bottom Line

The implementation is functionally close to the plan, but it has more lifecycle and state synchronization code than the feature currently needs. The clearest simplification is to make `localStore` queue rows the source of truth, derive UI state from them, and avoid global mutable services that must be manually wired to TinyBase.