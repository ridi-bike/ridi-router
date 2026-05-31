# Expo HAS CHANGED

Read the exact versioned docs at https://docs.expo.dev/versions/v56.0.0/ before writing any code.


# TinyBase Store Rules

## Store Split

- Use two TinyBase stores: `localStore` and `syncedStore`.
- `localStore` is device-local and must never sync.
- `syncedStore` is persisted locally now and reserved for future user-owned multi-device sync.
- Do not mix local-only operational data into `syncedStore`.
- Do not put future synced user-owned data into `localStore`.

## Store Ownership

- Components must not create TinyBase stores or persisters directly.
- Store creation, schema application, persistence, and startup reconciliation belong in `src/tinybase`.
- Feature code should use app-level TinyBase hooks/providers instead of importing store factories directly.
- Initialize TinyBase before rendering routes. If setup fails, show a root-level failure UI rather than partial app UI.

## Schema Rules

- Define schemas in Zod first, then convert them with TinyBase schematizers.
- Persisted cell and value names should use stable `snake_case`.
- Store dates as ISO strings, not `Date` objects.
- Prefer domain-derived row IDs. Use `ksuid` only when no stable domain ID exists.
- For RMDF tiles, the domain ID is `RmdfManifestTile.filename`.

## Local Store Rules

- Local store data may be hard-deleted.
- RMDF download state belongs in `localStore`.
- RMDF downloaded/queued/selected tile state should keep current UX behavior; only move durable state into TinyBase.
- The filesystem is authoritative for downloaded RMDF tile files.
- On startup, reconcile RMDF TinyBase state against the filesystem.
- If a downloaded RMDF file is missing, silently remove the stale local TinyBase state.
- Queued and partially downloaded RMDF work must survive app restart.
- The global downloading-files indicator should be driven by `localStore`, not separate React-only state.

## Synced Store Rules

- Synced store schema changes must be additive only after release.
- Never rename or remove synced tables, cells, values, or row ID formats.
- Never change the type or meaning of an existing synced field.
- Add only optional fields or fields with safe defaults.
- Synced domain entities must use soft deletes via `deleted_at`.
- Do not hard-delete synced domain entities.
- Treat unknown enum values or unknown fields as data to ignore, not fatal errors.
- Cloudflare Durable Object sync is the expected future sync direction, but sync implementation is currently out of scope.