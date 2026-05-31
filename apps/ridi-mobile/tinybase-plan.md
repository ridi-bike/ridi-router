# TinyBase Setup Plan

## Goals

Set up TinyBase as the app's local-first data layer with:

- Expo SQLite persistence.
- Zod-backed TinyBase schemas.
- A clean app-level provider API for React components.
- A schema evolution policy that keeps future synced data additive-only.
- No sync implementation yet, but no choices that block sync later.

## Current App Context

- Expo SDK: `~56.0.8`.
- React Native: `0.85.3`.
- Routing: `expo-router`.
- TypeScript strict mode is enabled.
- `expo-sqlite`, `tinybase`, and `zod` are not currently listed in `package.json`.

## Target Architecture

```text
React app
  |
  | TinyBaseProvider
  v
Typed TinyBase Store
  |
  | setSchema(from Zod schematizer)
  v
ExpoSqlitePersister
  |
  v
SQLite database file
```

## Dependencies

Install the required packages with Expo-compatible versions where applicable:

```sh
npx expo install expo-sqlite
pnpm add tinybase zod
```

Notes:

- Use `npx expo install` for `expo-sqlite` because this app is on Expo SDK 56.
- TinyBase's Expo SQLite persister imports from `tinybase/persisters/persister-expo-sqlite`.
- TinyBase's Zod schematizer imports from `tinybase/schematizers/schematizer-zod`.

## File Layout

Create a dedicated data module under `src/tinybase`:

```text
src/tinybase/
  schema.ts          # Zod schemas and TinyBase schema conversion
  store.ts           # Store factory and store singleton helpers
  persistence.ts     # SQLite database and ExpoSqlitePersister setup
  TinyBaseProvider.tsx
  hooks.ts           # App-specific typed hooks/re-exports
  ids.ts             # Table IDs, value IDs, and stable row ID helpers
```

Keep domain-specific schemas either in `schema.ts` initially or split later into:

```text
src/tinybase/schemas/
  rides.ts
  devices.ts
  settings.ts
```

## Store Type Decision

Use a regular TinyBase `Store` for the first implementation.

Reason:

- Current scope is local persistence only.
- TinyBase sync can use `MergeableStore` later for CRDT-style synchronization.
- The provider and persistence modules should hide store creation so the app can switch from `createStore()` to `createMergeableStore()` later with minimal surface-area change.

Future-proofing rule:

- Do not import `createStore()` directly from feature components.
- Components should consume the store through provider/hooks only.

## Schema Strategy

Define schemas in Zod first, then derive TinyBase schemas with `createZodSchematizer()`.

Example shape:

```ts
import {z} from 'zod';
import {createZodSchematizer} from 'tinybase/schematizers/schematizer-zod';

export const tableSchemas = {
  exampleEntities: z.object({
    name: z.string(),
    createdAt: z.string(),
    updatedAt: z.string().optional(),
    deletedAt: z.string().optional(),
  }),
};

export const valueSchemas = {
  schemaVersion: z.number().default(1),
};

const schematizer = createZodSchematizer();

export const tinybaseTablesSchema =
  schematizer.toTablesSchema(tableSchemas);

export const tinybaseValuesSchema =
  schematizer.toValuesSchema(valueSchemas);
```

Apply both schemas during store creation:

```ts
createStore().setSchema(tinybaseTablesSchema, tinybaseValuesSchema);
```

Guidelines:

- Use Zod for app-level validation before writing external or user-provided data.
- Use TinyBase schemas for store-level type/default enforcement.
- Prefer primitive cells: `string`, `number`, `boolean`, nullable values, arrays, and plain objects only where needed.
- Store dates as ISO strings, not `Date` objects, because unsupported Zod types may be omitted by the schematizer.

## Additive-Only Data Evolution Policy

Because future app versions will sync data between devices, old clients must tolerate data produced by newer clients.

Rules:

1. Never rename a table, row ID format, cell, or value ID.
2. Never remove a table, cell, or value that a released app may read.
3. Never change the meaning or type of an existing cell.
4. Only add new optional cells or cells with safe defaults.
5. Only add new tables that old app versions can ignore.
6. Do not add new required cells to existing tables unless every writer supplies a default that old readers can ignore.
7. Use tombstones instead of deletes for synced domain entities.
8. Keep enum-like fields backward-compatible by treating unknown values as `unknown` or ignoring the row in UI code.
9. Add migrations only for local repair/backfill, not for destructive reshaping of synced data.

Recommended baseline fields for synced rows:

```ts
{
  createdAt: z.string(),
  updatedAt: z.string().optional(),
  deletedAt: z.string().optional(),
  schemaVersion: z.number().default(1),
}
```

Use `deletedAt` as a tombstone for entities that may sync later. UI queries can filter tombstoned rows, but the data remains available for conflict resolution.

## Persistence Strategy

Use TinyBase's Expo SQLite persister in JSON serialization mode first.

Reason:

- It is the simplest mode.
- It persists the whole TinyBase store into a single SQLite table.
- It avoids early commitment to SQL table mappings.
- TinyBase documents JSON mode as compatible with regular `Store` and with persisting metadata for `MergeableStore`.

Database naming:

```ts
const DATABASE_NAME = 'ridi-tinybase.db';
const STORE_TABLE_NAME = 'tinybase_store';
```

Persister setup:

```ts
import {openDatabaseSync} from 'expo-sqlite';
import {createExpoSqlitePersister} from 'tinybase/persisters/persister-expo-sqlite';

const db = openDatabaseSync(DATABASE_NAME);
const persister = createExpoSqlitePersister(
  store,
  db,
  STORE_TABLE_NAME,
);
```

Startup sequence:

1. Create the store and apply schemas.
2. Open the SQLite database.
3. Create the Expo SQLite persister.
4. Load persisted content with `await persister.load()`.
5. Apply local migrations/backfills if needed.
6. Start auto-save with `await persister.startAutoSave()` or `await persister.startAutoPersisting()`.
7. Mark the TinyBase layer as ready for React rendering.

Shutdown/cleanup:

- Call `persister.destroy()` when the provider unmounts.
- Stop auto-save if the provider manages explicit lifecycle boundaries.

Development logging:

- Pass `onSqlCommand` and `onIgnoredError` only in development builds.
- Use the Expo SQLite devtools inspector to view the local database during development.

## Provider Plan

Add `TinyBaseProvider.tsx` that owns initialization and readiness.

Responsibilities:

- Create the store once.
- Create and own the persister once.
- Load persisted state before rendering children that depend on data.
- Expose loading and error states.
- Destroy the persister on unmount.

Integration point:

- Wrap the app in `app/_layout.tsx` inside existing global providers.
- Keep route components free of persistence setup.

Provider behavior:

```text
initializing -> load persisted store -> start auto-save -> ready
             -> error if load/setup fails
```

## Hook Plan

Create `src/tinybase/hooks.ts` for app usage.

Export only app-approved hooks, for example:

```ts
export {useCell, useRow, useTable, useValue} from 'tinybase/ui-react';
```

Then add typed helper hooks around common tables and values as the schema becomes concrete.

Rule:

- Feature code should not know how persistence is configured.
- Feature code should not create stores or persisters.

## Migration Plan

Use two separate migration concepts:

### SQLite container migration

Use SQLite `PRAGMA user_version` only for database-level concerns, such as:

- Enabling WAL.
- Changing the persistence table name.
- Adding future SQL-side support tables.

### TinyBase data migration

Use a TinyBase value such as `schemaVersion` for store data backfills.

Allowed migrations:

- Add missing default cells.
- Backfill new optional cells.
- Normalize invalid local-only values.

Disallowed migrations:

- Deleting old cells that synced clients may still use.
- Renaming fields in place.
- Converting a cell type in a way old app versions cannot read.

## Future Sync Compatibility

Do not implement sync now, but prepare for it:

- Centralize store creation so `Store` can become `MergeableStore` later.
- Use stable row IDs generated by app code, not array indexes or local counters.
- Include creation/update/tombstone metadata on domain rows.
- Avoid hard deletes for synced entities.
- Treat remote/unknown fields as valid data the current UI may ignore.
- Keep write APIs additive and defensive.

When sync is in scope, evaluate TinyBase `MergeableStore` and synchronizers before changing persistence.

## Implementation Order

1. Install dependencies.
2. Add `src/tinybase/schema.ts` with initial Zod schemas and schematizer conversion.
3. Add `src/tinybase/store.ts` with a single store factory.
4. Add `src/tinybase/persistence.ts` with SQLite and persister setup.
5. Add `src/tinybase/TinyBaseProvider.tsx` with async initialization.
6. Wrap the app in `app/_layout.tsx`.
7. Add `src/tinybase/hooks.ts` and migrate one small read/write path to prove the setup.
8. Add a development-only smoke test screen or log to confirm persistence survives app restart.
9. Document schema evolution rules near the schemas.

## Validation Checklist

- App starts with an empty database.
- Store schema defaults are applied.
- Data written through TinyBase appears after app restart.
- Provider does not render data-dependent children before load completes.
- Persistence errors are visible in development.
- No feature component creates a store or persister directly.
- New schema fields are optional or have safe defaults.
- Deletes for future-synced entities use tombstones.

## Open Decisions

- Initial domain tables and values.
- Whether to use TinyBase JSON persistence long-term or move to tabular mode for easier SQL inspection.
- Whether the first sync implementation should use TinyBase `MergeableStore` directly or another transport layer.
- How row IDs should be generated for each domain entity.
