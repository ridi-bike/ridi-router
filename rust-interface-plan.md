# Rust Routing Interface Plan

## Goal

Expose `crates/ridi-router-routing` to `apps/ridi-mobile` through an Expo local module:

```text
React / TypeScript
  -> Expo local module
  -> Swift / Kotlin glue
  -> UniFFI-generated bindings
  -> Rust mobile facade crate
  -> ridi-router-routing
```

Initial scope is route generation only:

- Input: absolute local tile directory path and route request JSON.
- Output: JSON value containing either route computation or structured error.
- Execution: Expo `AsyncFunction`.
- Excluded: tile downloading, tile directory setup, storage-location decisions, app state management, route-parameter selection UI, route progress events, cancellation.

The module assumes another feature has already downloaded/prepared a local tile directory containing `manifest.json` and referenced `.rmdf` files.

## Key Decisions

1. Use an Expo local module in `apps/ridi-mobile/modules/`.
2. Add a new Rust facade crate instead of exposing `ridi-router-routing` directly.
3. Use UniFFI for Swift/Kotlin bindings.
4. Use a JSON boundary.
5. Match Rust serde request/response shapes exactly.
6. Do not add request field mapping, rule defaulting, or behavior changes in the module.
7. Use Expo `AsyncFunction`, not synchronous `Function`.
8. Return all expected errors as structured values, not rejected promises or thrown errors.
9. Open `RoutingExecutor` per call; no caching or persistent router session in v1.
10. Support all Android ABIs.
11. Package iOS Rust output as `.xcframework`, including device, Apple Silicon simulator, and Intel simulator slices.
12. Use `./dev.sh` for Rust builds, artifact copying, and Expo launch helpers.
13. Keep generated native `android/` and `ios/` app folders disposable; persistent source should live in the Expo module, Rust crates, and `./dev.sh`.

## Proposed File Layout

```text
./dev.sh

crates/ridi-router-mobile/
  Cargo.toml
  build.rs
  uniffi-bindgen.rs
  src/
    lib.rs
    ridi_router_mobile.udl

apps/ridi-mobile/modules/ridi-router/
  expo-module.config.json
  src/
    index.ts
    RidiRouter.types.ts
  android/
    build.gradle
    src/main/java/.../RidiRouterModule.kt
    src/main/java/uniffi/ridi_router_mobile/
    src/main/jniLibs/<abi>/libridi_router_mobile.so
  ios/
    RidiRouterModule.swift
    RidiRouter.podspec
    rust/
      RidiRouterMobile.xcframework
      generated Swift bindings
```

Exact module names can be adjusted before scaffolding, but should stay stable after native files are generated.

## Rust Facade API

Create `crates/ridi-router-mobile` as a thin wrapper around `ridi-router-routing`.

Expose one UniFFI function for v1:

```text
namespace ridi_router_mobile {
  string generate_route_json(string tiles_dir, string request_json);
}
```

No `default_rules_json` helper for v1. The app must construct the full request JSON it wants to send.

## Request JSON Contract

`request_json` must match the serde shape of `ridi_router_routing::RouteRequest` exactly.

Start/finish request:

```json
{
  "mode": {
    "StartFinish": {
      "start": { "lat": 48.1372, "lon": 11.5761 },
      "finish": { "lat": 48.2, "lon": 11.7 }
    }
  },
  "rules": {
    "basic": {},
    "highway": null,
    "surface": null,
    "smoothness": null,
    "generation": {}
  }
}
```

Round-trip request:

```json
{
  "mode": {
    "RoundTrip": {
      "start_finish": { "lat": 48.1372, "lon": 11.5761 },
      "bearing": 90.0,
      "distance": 25000
    }
  },
  "rules": {
    "basic": {},
    "highway": null,
    "surface": null,
    "smoothness": null,
    "generation": {}
  }
}
```

Rules behavior must be the existing `RouterRules` serde behavior only:

- The module must not invent default rules.
- The module must not rename fields.
- The module must not translate app-friendly request shapes into Rust shapes.
- If the app wants friendlier shapes, the app layer handles that separately.
- If the routing crate already applies serde defaults for nested fields, those remain part of the routing crate behavior.

## Response JSON Contract

`generate_route_json` always returns a JSON string containing a result envelope.

Success:

```json
{
  "ok": true,
  "value": {
    "routes": [
      {
        "coords": [[48.1372, 11.5761]],
        "stats": {}
      }
    ]
  }
}
```

Error:

```json
{
  "ok": false,
  "error": {
    "code": "invalid_request_json",
    "message": "Failed to parse request JSON",
    "details": "..."
  }
}
```

The `value` field is the existing serde output of `ridi_router_routing::RouteComputation`.

Expected errors should never throw through UniFFI and should never reject the JS promise. The Rust facade, Kotlin glue, and Swift glue should catch expected failures and return an error envelope.

Unhandled process-level failures, native crashes, or programming bugs may still crash or reject; those are not part of the expected error contract.

## Error Codes

Use stable structured codes for expected failures:

```text
invalid_request_json
invalid_tiles_dir
routing_open_failed
route_generation_failed
response_serialization_failed
native_binding_failed
unknown
```

Notes:

- `invalid_request_json`: JSON is malformed or does not deserialize into `RouteRequest`.
- `invalid_tiles_dir`: path is empty, not absolute, not a directory, or lacks `manifest.json`.
- `routing_open_failed`: `RoutingExecutor::open` failed.
- `route_generation_failed`: `RoutingExecutor::generate` failed.
- `response_serialization_failed`: result envelope or route computation could not serialize.
- `native_binding_failed`: Kotlin/Swift caught an unexpected binding-layer exception.
- `unknown`: last-resort fallback.

## Rust Facade Behavior

For each call:

1. Receive `tiles_dir` and `request_json`.
2. Validate `tiles_dir` enough to return `invalid_tiles_dir` for obvious local path problems.
3. Deserialize `request_json` directly into `ridi_router_routing::RouteRequest`.
4. Open `RoutingExecutor` using `RoutingExecutorConfig { tiles_dir }`.
5. Call `generate(request, None)`.
6. Serialize `RouteComputation` into the success envelope.
7. Return the envelope JSON string.

No executor cache, global session, or multi-directory registry in v1.

## TypeScript Surface

Expose the native module through a small TypeScript wrapper.

Recommended types:

```ts
export type RidiRouterErrorCode =
  | 'invalid_request_json'
  | 'invalid_tiles_dir'
  | 'routing_open_failed'
  | 'route_generation_failed'
  | 'response_serialization_failed'
  | 'native_binding_failed'
  | 'unknown';

export type RidiRouterError = {
  code: RidiRouterErrorCode;
  message: string;
  details?: string;
};

export type RidiRouterResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: RidiRouterError };

export async function generateRouteJson(
  tilesDir: string,
  requestJson: string,
): Promise<RidiRouterResult<unknown>>;
```

The wrapper may parse the returned envelope string into `RidiRouterResult<unknown>`, but it must not map app-friendly request shapes to Rust shapes.

If a native promise rejects unexpectedly, the wrapper should catch it and return:

```ts
{
  ok: false,
  error: {
    code: 'native_binding_failed',
    message: 'Native routing call failed',
    details: String(error),
  },
}
```

## Expo Local Module Work

1. From `apps/ridi-mobile`, create a local module:

   ```bash
   npx create-expo-module@latest --local --platform android apple --features AsyncFunction
   ```

2. Name it something like:

   - package directory: `modules/ridi-router`
   - native module name: `RidiRouter`
   - Android package: `bike.ridi.modules.router`

3. Expose one async function initially:

   ```ts
   generateRouteJson(tilesDir: string, requestJson: string): Promise<string>
   ```

4. Keep native module code minimal:

   - Kotlin calls UniFFI `generateRouteJson(tilesDir, requestJson)`.
   - Swift calls UniFFI `generateRouteJson(tilesDir:requestJson:)`.
   - Kotlin/Swift catches binding-layer exceptions and returns a structured error envelope string.

## Android Build Plan

### Required Rust targets

Support all Android ABIs:

```bash
rustup target add \
  aarch64-linux-android \
  armv7-linux-androideabi \
  i686-linux-android \
  x86_64-linux-android
```

Also install:

```bash
cargo install cargo-ndk
```

### ABI mapping

```text
aarch64-linux-android     -> arm64-v8a
armv7-linux-androideabi  -> armeabi-v7a
i686-linux-android       -> x86
x86_64-linux-android     -> x86_64
```

### Build output

Compile `crates/ridi-router-mobile` as a `cdylib` and copy outputs to:

```text
apps/ridi-mobile/modules/ridi-router/android/src/main/jniLibs/<abi>/libridi_router_mobile.so
```

### Kotlin bindings

Generate UniFFI Kotlin bindings and copy them into the local module Android source tree:

```text
apps/ridi-mobile/modules/ridi-router/android/src/main/java/uniffi/ridi_router_mobile/
```

### Android dependencies

Add JNA to the Expo module Gradle file if required by generated UniFFI bindings:

```gradle
implementation "net.java.dev.jna:jna:5.13.0@aar"
```

Use the same Android platform level as the app min SDK. The generated Expo app appears to use min SDK 24, so build Rust with platform 24 unless the app config says otherwise.

## iOS Build Plan

### Required Rust targets

Install:

```bash
rustup target add \
  aarch64-apple-ios \
  aarch64-apple-ios-sim \
  x86_64-apple-ios
```

### Build output

Compile `crates/ridi-router-mobile` as a `staticlib` and package it as:

```text
RidiRouterMobile.xcframework
```

Include all of these slices:

```text
aarch64-apple-ios       # iOS device
aarch64-apple-ios-sim   # Apple Silicon simulator
x86_64-apple-ios        # Intel simulator
```

Copy the `.xcframework` to:

```text
apps/ridi-mobile/modules/ridi-router/ios/rust/RidiRouterMobile.xcframework
```

### Swift bindings

Generate UniFFI Swift bindings and include them in the Expo module iOS target.

### Podspec

Update the local module podspec to vend the xcframework:

```ruby
s.vendored_frameworks = 'rust/RidiRouterMobile.xcframework'
```

## UniFFI Versioning

Use the current compatible stable UniFFI version at implementation time and pin it exactly in `crates/ridi-router-mobile/Cargo.toml` and `Cargo.lock`.

The version must support:

- Rust scaffolding generation from UDL.
- Kotlin binding generation.
- Swift binding generation.
- Android/iOS targets used by this project.

Generated Kotlin/Swift bindings must be regenerated whenever the UDL changes.

## `./dev.sh` Plan

Use the root-level `./dev.sh` as the entry point for repeated native work.

Required command groups:

```bash
./dev.sh rust-router android
./dev.sh rust-router ios
./dev.sh rust-router bindings
./dev.sh rust-router build

./dev.sh mobile android device
./dev.sh mobile android simulator
./dev.sh mobile ios device
./dev.sh mobile ios simulator
```

### `./dev.sh rust-router android`

Should:

1. Check required Android Rust targets.
2. Check `cargo-ndk` is installed.
3. Build all Android ABI `.so` files.
4. Copy each `.so` into the matching Expo module `jniLibs/<abi>/` directory.

### `./dev.sh rust-router ios`

Should:

1. Check required iOS Rust targets.
2. Build static libraries for device, Apple Silicon simulator, and Intel simulator.
3. Create `RidiRouterMobile.xcframework`.
4. Copy it into the Expo module iOS `rust/` directory.

### `./dev.sh rust-router bindings`

Should:

1. Generate Kotlin bindings.
2. Generate Swift bindings.
3. Copy generated bindings into the Expo module native source trees.

### `./dev.sh rust-router build`

Should run:

1. `rust-router bindings`
2. `rust-router android`
3. `rust-router ios`

### `./dev.sh mobile ...`

Should launch the Expo app on the requested platform/target.

Implementation can wrap existing Expo commands, for example:

```bash
pnpm --dir apps/ridi-mobile android
pnpm --dir apps/ridi-mobile ios
```

Device vs simulator flags should be mapped to Expo CLI behavior during implementation.

## Artifact Policy

Rust native artifacts are generated and copied by `./dev.sh`.

Source of truth:

- Rust crates
- UDL file
- Expo local module source
- `./dev.sh`

Generated artifacts:

- Android `.so` files
- iOS `.xcframework`
- UniFFI Kotlin bindings
- UniFFI Swift bindings

Whether generated artifacts are committed can be decided later based on release/EAS workflow. The implementation should not require manual copying.

## Tile Directory Contract

The module receives an absolute local filesystem path.

The directory must contain:

```text
manifest.json
tile_*.rmdf
```

The module does not:

- download tiles
- choose storage location
- copy bundled assets
- manage SD card vs internal storage
- select tile regions
- decide when routing is enabled

Those concerns belong to separate map/state/download work.

## Testing Plan

### Rust unit tests

Add tests in `crates/ridi-router-mobile` for:

- invalid request JSON
- request JSON matching `RouteRequest` serde shape
- missing or invalid tiles directory
- routing open failure envelope
- route generation failure envelope
- successful generation using existing synthetic tile fixtures where practical
- response envelope serialization

### Native module smoke tests

Android:

- Build Rust `.so` files for all ABIs.
- Verify app launches with the module linked.
- Call `generateRouteJson` with a known local tile fixture.
- Verify errors resolve as `{ ok: false, error }`, not rejected promises.

IOS:

- Build full xcframework.
- Run app on simulator.
- Call `generateRouteJson` with a known local tile fixture.
- Verify errors resolve as `{ ok: false, error }`, not rejected promises.

### TypeScript tests

Test the JS wrapper only:

- returned envelope parsing
- unexpected native rejection conversion to `native_binding_failed`
- no request-shape mapping in the wrapper

## Build / Dev Workflow

Expected local loop:

```bash
# Build Rust artifacts, generate UniFFI bindings, copy artifacts
./dev.sh rust-router build

# Reinstall pods after iOS native module changes if needed
pnpm --dir apps/ridi-mobile exec pod-install

# Run development builds
./dev.sh mobile android simulator
./dev.sh mobile ios simulator
```

Native changes require rebuilding the development app. JS-only wrapper changes should be picked up by Metro.

## Acceptance Criteria

1. `apps/ridi-mobile` has a local Expo module exposing async route generation.
2. `crates/ridi-router-mobile` builds for all Android ABIs and required iOS slices.
3. `./dev.sh` builds Rust artifacts and copies them into the Expo local module.
4. `./dev.sh` can launch the Expo app on Android/iOS device or simulator targets.
5. Android app links the Rust `.so` libraries successfully.
6. iOS app links `RidiRouterMobile.xcframework` successfully.
7. TypeScript can call `generateRouteJson(tilesDir, requestJson)`.
8. Successful calls return `{ ok: true, value: RouteComputation }` JSON.
9. Expected failures return `{ ok: false, error: { code, message, details? } }` JSON.
10. Expected failures do not reject promises or throw.
11. No route progress events are required.
12. No tile download, storage setup, route selection UI, or app state integration is included.
13. Generated `android/` and `ios/` app folders remain disposable under Expo prebuild.

## Risks and Follow-ups

### Large JSON payloads

Routes may be large. JSON is acceptable for v1, but if performance becomes an issue, consider typed UniFFI records or compact binary output later.

### Repeated executor opening

Opening `RoutingExecutor` on every request is simple and intentionally chosen for v1. If it proves too slow, add a native/Rust session object later.

### ABI drift

Android ABI support must match the app's `reactNativeArchitectures`. V1 builds all Android ABIs to avoid mismatch.

### iOS packaging

Use `.xcframework` from the start to avoid device/simulator selection problems.

### Threading

Route generation uses Rayon internally. Keep Expo calls async and avoid adding callbacks until threading behavior is fully tested.

### Expo native rebuilds

Changes to Rust artifacts, UniFFI bindings, Kotlin, Swift, Gradle, or Podspec files require rebuilding the development app.
