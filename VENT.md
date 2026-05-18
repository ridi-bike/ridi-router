# VENT

Feedback log. Repeated/systemic workflow friction that should become future automation, docs, or workflow fixes.

## 26-05-04 22:28 — tool_error

The installed `osmium` binary failed to start because `libboost_program_options.so.1.89.0` is missing. I worked around it by generating the tiny OSM PBF fixture directly with a Python protobuf encoder. Keeping osmium's runtime libraries in sync, or documenting a supported fixture-generation command/container, would avoid this manual workaround next time.
## 26-05-23 18:36 — simpleperf binary cache lock-file failure

Android simpleperf recording succeeds, but app_profiler post-processing repeatedly fails while scanning target/android-ridi-app/target/aarch64-linux-android/debug/incremental/*.lock files owned by root from Docker cargo-apk builds. Workaround is to ignore the app_profiler exit and run report.py directly against perf.data. A dedicated profiling script should pass only exact .so/debug-symbol paths or exclude incremental dirs so profiling runs don't appear failed after successful recording.
