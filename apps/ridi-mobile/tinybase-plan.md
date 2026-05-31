# TinyBase Setup Plan

## Goals

Set up TinyBase as the app-wide state layer with two stores:

- `localStore`: persisted locally, never synced.
- `syncedStore`: persisted locally now, designed for future sync.

Initial implementation will use the local store for RMDF download state. Sync behavior is out of scope, but the plumbing for a future synced store must exist from the start.

## Current App Context

- Expo SDK: `~56.0.8`.
- React Native: `0.85.3`.
- Routing: `expo-router`.
- TypeScript strict mode is enabled.
- Existing RMDF code lives under `src/rmdf`.
- `expo-sqlite`, `tinybase`, and `zod` need to be added.

## Target Architecture

```text
App startup
  |
  v
Initialize TinyBase before route rendering
  |
  +-- localStore
  |     |
  |     +-- Expo SQLite persister
  |     +-- RMDF filesystem reconciliation
  |
  +-- syncedStore
        |
        +-- Expo SQLite persister now
        +-- future Cloudflare Durable Object sync
```

## Dependencies

Install:

```sh
npx expo install expo-sqlite
pnpm add tinybase zod ksuid
```

Notes:

- Use `npx expo install` for `expo-sqlite` because this app is on Expo SDK 56.
- Use TinyBase Expo SQLite persistence from `tinybase/persisters/persister-expo-sqlite`.
- Use the Zod schematizer from `tinybase/schematizers/schematizer-zod`.
- Use domain-derived row IDs where possible. Use `ksuid` only when there is no stable domain ID.

## Store Responsibilities

### Local Store

Use for device-local state that will never sync.

Initial responsibilities:

- RMDF tile download queue.
- RMDF downloaded tile tracking.
- RMDF selected/active tile state.
- Local-only operational state that can be rebuilt or deleted without affecting other devices.

Rules:

- Hard deletes are allowed.
- It may mirror filesystem state, but the filesystem remains authoritative for actual downloaded tile files.
- On startup, TinyBase state must be reconciled with the filesystem so stale rows are removed or marked accurately.

### Synced Store

Use for user-owned data that should eventually sync between the user's devices.

Initial responsibilities:

- No domain features yet.
- Create the store, schema, provider plumbing, and persistence now.

Future sync target:

- User-owned data synced between multiple devices through a central Cloudflare Durable Object sync service.
- Shared multi-user state is out of scope for now.

Rules:

- Use soft deletes with `deleted_at`.
- Do not hard-delete synced domain entities.
- Prefer additive-only schema changes.
- Treat unknown future fields and values as data to ignore, not as errors.

## File Layout

```text
src/tinybase/
  ids.ts
  TinyBaseProvider.tsx
  hooks.ts

  local/
    schema.ts
    store.ts
    persistence.ts
    rmdfReconcile.ts

  synced/
    schema.ts
    store.ts
    persistence.ts
```

Keep route and feature components unaware of store creation and persistence details.

## Startup Sequence

TinyBase must load before the app renders routes so screens do not need TinyBase loading states.

1. Create both stores.
2. Apply Zod-derived TinyBase schemas.
3. Open both SQLite databases.
4. Create both Expo SQLite persisters.
5. Load both persisted stores.
6. Reconcile local RMDF TinyBase state with the filesystem.
7. Start auto persistence for both stores.
8. Render the app.

If initialization fails, render a root-level fatal error screen instead of partial app UI.

## Persistence Strategy

Use one SQLite database per store.

```ts
const LOCAL_DATABASE_NAME = 'ridi-local-tinybase.db';
const SYNCED_DATABASE_NAME = 'ridi-synced-tinybase.db';

const LOCAL_STORE_TABLE_NAME = 'local_store';
const SYNCED_STORE_TABLE_NAME = 'synced_store';
```

Use TinyBase Expo SQLite persister JSON mode initially.

Reasons:

- Simple setup.
- Good fit while schemas are still small.
- Avoids early SQL table mapping decisions.
- Keeps a future switch to `MergeableStore` contained inside store setup.

Use development-only logging hooks for SQL and ignored persistence errors.

## Schema Strategy

Define Zod schemas first, then convert them to TinyBase schemas.

```ts
import {z} from 'zod';
import {createZodSchematizer} from 'tinybase/schematizers/schematizer-zod';

const schematizer = createZodSchematizer();

export const tinybaseTablesSchema = schematizer.toTablesSchema(tableSchemas);
export const tinybaseValuesSchema = schematizer.toValuesSchema(valueSchemas);
```

Guidelines:

- Store dates as ISO strings.
- Use snake_case for persisted cell names so future sync payloads are stable and platform-neutral.
- Avoid Zod `date()` in TinyBase-backed schemas.
- Use Zod validation before writing untrusted or external data.
- Use TinyBase schemas for runtime defaults and basic cell validation.

## Initial Local Schema

Purpose: track RMDF download state while reconciling with actual tile files.

### Tables

#### `rmdf_tiles`

One row per known RMDF tile.

Row ID:

- Domain-derived tile ID.
- Prefer an existing stable RMDF tile key if one exists.
- Otherwise use a deterministic ID from tile coordinates or tile URL parts.

Cells:

```ts
rmdf_tiles: z.object({
  tile_id: z.string(),
  source_id: z.string().optional(),
  url: z.string().optional(),
  local_path: z.string().optional(),

  status: z.string().default('known'),
  // known | queued | downloading | downloaded | failed | missing

  selected: z.boolean().default(false),

  byte_size: z.number().optional(),
  downloaded_at: z.string().optional(),
  last_checked_at: z.string().optional(),
  error_message: z.string().optional(),
})
```

Notes:

- `downloaded` means TinyBase believes the file exists.
- Filesystem reconciliation must verify that `local_path` still exists.
- If a file is missing, either hard-delete the row or set `status = 'missing'` depending on UX needs.
- The actual file remains outside TinyBase.

#### `rmdf_download_queue`

One row per queued or active download job.

Row ID:

- Domain-derived from tile ID, for example `tile:${tile_id}`.

Cells:

```ts
rmdf_download_queue: z.object({
  tile_id: z.string(),
  priority: z.number().default(0),
  state: z.string().default('queued'),
  // queued | running | paused | failed

  attempts: z.number().default(0),
  queued_at: z.string(),
  started_at: z.string().optional(),
  updated_at: z.string().optional(),
  error_message: z.string().optional(),
})
```

Notes:

- Completed jobs should be hard-deleted from this local-only queue.
- Failed jobs can remain until retried or dismissed.

### Values

```ts
localValues: {
  local_schema_version: z.number().default(1),
  selected_rmdf_tile_id: z.string().optional(),
  rmdf_last_reconciled_at: z.string().optional(),
}
```

## Initial Synced Schema

Purpose: establish plumbing and conventions before synced features exist.

### Tables

Start with no feature tables unless a concrete synced feature is added.

Add a reserved metadata table only if needed by implementation:

```ts
sync_metadata: z.object({
  key: z.string(),
  value: z.string().optional(),
  updated_at: z.string().optional(),
})
```

Prefer TinyBase values for simple metadata.

### Values

```ts
syncedValues: {
  synced_schema_version: z.number().default(1),
  sync_client_id: z.string().optional(),
  last_successful_sync_at: z.string().optional(),
}
```

Future synced entity baseline:

```ts
{
  created_at: z.string(),
  updated_at: z.string().optional(),
  deleted_at: z.string().optional(),
  schema_version: z.number().default(1),
}
```

## RMDF Filesystem Reconciliation

TinyBase must not blindly trust persisted RMDF download state.

On startup after loading `localStore`:

1. Read known RMDF download directory contents.
2. Build a set of actual tile file paths or tile IDs.
3. For every `rmdf_tiles` row with `status = 'downloaded'`:
   - if `local_path` exists, update `last_checked_at`;
   - if missing, set `status = 'missing'` or delete the row.
4. For files found on disk but missing from TinyBase:
   - add a `rmdf_tiles` row if the tile ID can be derived;
   - otherwise ignore or record a repair warning.
5. Remove queue rows for tiles that are already downloaded.
6. Set `rmdf_last_reconciled_at`.

Open decision:

- Should missing files be shown as `missing` for user awareness, or should local state be hard-deleted silently?

## Additive-Only Synced Data Policy

For the synced store:

1. Never rename a table, row ID format, cell, or value ID after release.
2. Never remove a table, cell, or value that a released app may read.
3. Never change the meaning or type of an existing cell.
4. Only add optional cells or cells with safe defaults.
5. Only add new tables that old app versions can ignore.
6. Do not add new required fields to existing synced tables.
7. Use `deleted_at` tombstones instead of hard deletes.
8. Treat unknown enum-like values as unknown/unsupported, not fatal.
9. Use migrations only for additive backfills and local repair.

The local store does not need this full policy because it is never synced, but stable schemas are still preferred.

## Derived and Cache Data

Derived/cache data means data that can be recreated from another source of truth.

Examples that may fit the local store:

- Download queue rows derived from user actions.
- Tile file existence derived from the filesystem.
- Last filesystem reconciliation timestamp.
- Cached tile byte sizes from file metadata.
- Last download error text.
- UI selection state such as selected RMDF tile.

Examples that should usually not go in synced store:

- Render-only map viewport state.
- Temporary loading flags.
- Rebuildable aggregates.
- Network request progress.
- Data copied from server responses if it can be fetched again and is not user-owned.

Rule:

- Put rebuildable operational state in `localStore`.
- Put user-authored or user-owned durable state in `syncedStore`.

## Store Creation Rules

- Components must not call `createStore()` directly.
- Components must not create persisters.
- Components access stores through `TinyBaseProvider` and app hooks.
- Store creation should be centralized so `syncedStore` can become a TinyBase `MergeableStore` later.

## Provider Plan

`TinyBaseProvider` owns both stores and both persisters.

Responsibilities:

- Initialize stores before rendering children.
- Persist both stores.
- Run RMDF filesystem reconciliation for local store.
- Expose stores through context.
- Destroy persisters on unmount.
- Show root-level initialization failure if setup fails.

## Hook Plan

Create `src/tinybase/hooks.ts` with app-approved accessors:

- `useLocalStore()`
- `useSyncedStore()`
- `useLocalCell(...)`
- `useSyncedCell(...)`
- RMDF-specific hooks as needed, such as `useRmdfTile(tileId)`.

Feature code should use these wrappers instead of importing TinyBase primitives directly unless there is a clear reason.

## Migration Plan

### SQLite container migrations

Use SQLite `PRAGMA user_version` only for database-level concerns:

- WAL mode.
- Persistence table changes.
- Future SQL helper tables.

### TinyBase store migrations

Use TinyBase values:

- `local_schema_version`
- `synced_schema_version`

Local migrations may hard-delete invalid local-only rows.

Synced migrations must be additive only.

## Future Sync Plan

When sync becomes in scope:

1. Evaluate switching `syncedStore` from `Store` to `MergeableStore`.
2. Keep `localStore` as a regular non-syncing store.
3. Persist `syncedStore` locally before and after sync.
4. Add Cloudflare Durable Object sync transport.
5. Keep soft delete and additive schema rules.
6. Add sync conflict tests before shipping.

## Implementation Order

1. Install dependencies.
2. Add local and synced schema modules.
3. Add local and synced store factories.
4. Add local and synced persistence setup.
5. Add RMDF filesystem reconciliation.
6. Add `TinyBaseProvider` and block app rendering until initialization completes.
7. Wrap `app/_layout.tsx` with the provider.
8. Add hooks.
9. Migrate RMDF download state into `localStore`.
10. Verify persisted RMDF state survives restart.
11. Verify deleted tile files are detected on startup.

## Validation Checklist

- App does not render routes until both TinyBase stores are loaded.
- Local store persists across app restarts.
- Synced store persists locally even though sync is not active.
- RMDF tile rows are reconciled against real files on startup.
- Queue rows are removed for already-downloaded tiles.
- Missing files do not leave stale `downloaded` state.
- Components do not create stores or persisters.
- Local hard deletes are allowed.
- Synced entities use `deleted_at` soft deletes.
- Domain-derived IDs are used where available.
- `ksuid` is available for entities without domain IDs.

## Remaining Questions

1. What is the exact RMDF tile identity format available today?
2. Should missing RMDF files be shown to the user as `missing`, or should rows be hard-deleted silently?
3. Should `selected_rmdf_tile_id` allow only one selected tile, or can multiple tiles be selected at once?
4. Should queued downloads survive app restart, or should the queue be rebuilt from selected/missing tiles?
5. Should download progress percentages be persisted, or treated as transient UI state?
