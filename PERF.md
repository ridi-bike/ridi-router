# Performance Profiling

## Android app CPU profiling

Use Android `simpleperf` from the NDK. Host `perf`/`flamegraph` will not see CPU inside the Android app process.

## Prerequisites

- Phone connected over adb:

```bash
adb devices
```

- Debug APK must be Android-debuggable. The app manifest is generated from `crates/ridi-app/Cargo.toml`; make sure debug builds include:

```toml
[package.metadata.android.application_attributes]
"android:debuggable" = "true"
```

Verify after install:

```bash
adb shell dumpsys package bike.ridi.app | grep -Ei 'flags=|debug|profile'
```

Expected:

```text
flags=[ DEBUGGABLE HAS_CODE ... ]
```

## Build, install, launch

```bash
./dev.sh app-build-debug
adb install -r target/android-ridi-app/target/android-artifacts/debug/apk/app.apk
adb logcat -c
adb shell monkey -p bike.ridi.app -c android.intent.category.LAUNCHER 1
```

## Record a profile

Create an output directory:

```bash
RUN_DIR="profiling/simpleperf-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$RUN_DIR"
echo "$RUN_DIR" > profiling/latest-run-dir.txt
OUT_DIR="$PWD/$RUN_DIR"
```

Run a 90-second recording:

```bash
cd "$OUT_DIR"
python3 /opt/android-ndk/simpleperf/app_profiler.py \
  -p bike.ridi.app \
  -r "-e task-clock:u -f 997 --duration 90 -g" \
  -lib /home/toms/dev/ridi-router/target/android-ridi-app/target/android-artifacts/debug \
  -o "$OUT_DIR/perf.data" \
  > "$OUT_DIR/app_profiler.log" 2>&1
```

Interact with the app while recording.

## Generate reports

```bash
OUT_DIR="$PWD/$(cat profiling/latest-run-dir.txt)"

python3 /opt/android-ndk/simpleperf/report.py \
  -i "$OUT_DIR/perf.data" \
  --children \
  > "$OUT_DIR/report-children.txt" \
  2> "$OUT_DIR/report-children.err"

python3 /opt/android-ndk/simpleperf/report.py \
  -i "$OUT_DIR/perf.data" \
  -g --children \
  > "$OUT_DIR/report-callgraph.txt" \
  2> "$OUT_DIR/report-callgraph.err"

python3 /opt/android-ndk/simpleperf/report_html.py \
  -i "$OUT_DIR/perf.data" \
  -o "$OUT_DIR/report.html" \
  > "$OUT_DIR/report_html.log" 2>&1
```

Useful quick search:

```bash
rg -n "draw_map_tiles|draw_styled_feature_fill|triangulate|update_visible_tiles|draw_triangle|FillGeometryCache" "$OUT_DIR/report-children.txt"
```

## Known simpleperf post-processing issue

`app_profiler.py` can successfully record `perf.data` but still exit non-zero during binary-cache generation if it scans root-owned Docker build files, for example:

```text
PermissionError: ... target/aarch64-linux-android/debug/incremental/...lock
```

If `perf.data` exists, continue and run `report.py` / `report_html.py` manually as above.

## Reading results

Important columns:

- `Children`: time in the function and its callees.
- `Self`: time directly in that function.

For this immediate-mode app, high `Children` under `draw_map_tiles` is expected. Look for expensive callees beneath it.

Recent findings:

- Per-frame tile set recomputation was reduced by caching the visible tile rectangle.
- Fill triangulation cache works when overlay shows `misses=0`.
- If frame time remains high with `FillGeometryCache` hits high and misses zero, the bottleneck is drawing/converting cached triangles, not triangulation.

## On-screen debug overlay

The app has a temporary performance/touch overlay showing:

- current frame time
- approximate FPS
- max frame time in the last second
- active touches
- last recognized gesture
- fill geometry cache hits/misses/features/tiles

Interpretation:

- `misses > 0` on a static viewport means geometry cache churn or bad cache keys.
- `misses = 0` and high frame time means cached rendering is still too expensive.
- Very high frame time, e.g. `2700ms`, means touch handling will feel broken because input is sampled only once per frame.
