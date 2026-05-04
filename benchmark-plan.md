# Roundabout benchmark plan

_Date:_ 2026-04-11

## Purpose

Do **not** change roundabout code yet.
First benchmark roundabout-specific paths so future work has clean signal.

Target functions:
- `walker::with_roundabout_exits_for_segment_with_context`
- `walker::move_to_roundabout_exit_with_context`

Related code:
- `crates/ridi-router-routing/src/router/walker.rs`

## Why separate benchmark plan exists

Earlier POCs mixed:
- forward walker visited tracking
- roundabout visited tracking
- other walker micro-changes

That blurred attribution.
Need isolated evidence before touching roundabout code.

## Benchmark route

Use same route as all prior perf work so results stay comparable between runs:

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

Reason:
- already baseline-profiled
- includes several roundabouts
- keeps future roundabout experiments directly comparable with prior walker work

## Goal

Answer these questions before any roundabout optimization:
1. Do roundabout helpers show up materially on current benchmark route?
2. Which helper costs more on this route:
   - `with_roundabout_exits_for_segment_with_context`
   - `move_to_roundabout_exit_with_context`
3. Does changing roundabout visited tracking improve total hotpath, or only move cost around locally?
4. Do roundabout-specific changes affect route output?

## Limitation

This route is not guaranteed to be maximally roundabout-heavy.
That is acceptable for now.

Decision:
- optimize for comparability first
- if roundabout helpers stay too small on this route, postpone roundabout work instead of inventing a special benchmark route

## Variants to benchmark later

Benchmark one change at a time.

### Variant A — baseline
No code changes.

### Variant B — visited tracker only in `with_roundabout_exits_for_segment_with_context`
Replace only roundabout exit helper visited structure.

### Variant C — visited tracker only in `move_to_roundabout_exit_with_context`
Replace only append-to-exit helper visited structure.

### Variant D — both roundabout helpers changed together
Only after B and C measured independently.

## Metrics to capture

For each variant:
- total hotpath elapsed
- wrapper wall
- timing rows for:
  - `walker::move_forward_to_next_fork_with_context`
  - `walker::with_fork_segments_for_segment_with_context`
  - `walker::with_roundabout_exits_for_segment_with_context` if visible
  - `walker::move_to_roundabout_exit_with_context` if visible
  - `weights::weight_heading`
- alloc rows for same functions when available
- route count / GPX output count
- any output drift or behavioral anomalies

## Success criteria for future roundabout change

Keep roundabout optimization only if:
- target roundabout helper improves clearly
- total hotpath also improves, not only local helper time
- route outputs stay same
- no regression against current baseline route

## Benchmark method

For each future roundabout variant:
1. use same command
2. collect **3 warm runs**
3. compare median hotpath elapsed
4. compare target roundabout helper totals
5. verify route output parity

## Recording format

When future roundabout benchmarking starts, create files like:
- `.pi/perf/roundabout-baseline.md`
- `.pi/perf/roundabout-helper-exits.md`
- `.pi/perf/roundabout-helper-move-exit.md`
- `.pi/perf/roundabout-both.md`

Each file should include:
- command
- run log path(s)
- hotpath elapsed
- relevant timing rows
- relevant alloc rows
- verdict

## First concrete next step for roundabouts

Not code.

First do baseline inspection on same route:
1. verify whether roundabout helpers are visible enough in hotpath output
2. if yes, benchmark variants B and C separately later
3. if no, postpone roundabout optimization and stay focused on forward walker work
