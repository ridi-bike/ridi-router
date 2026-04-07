# Route generation performance report

## Exact command run

```bash
RIDI_FEATURES=perf ./dev.sh route riga,latvia sigulda,latvia
```

## What `dev.sh` resolved and ran

- preset: `default`
- rule file: `rule-examples/rules-default.json`
- start query: `riga,latvia`
- finish query: `sigulda,latvia`
- resolved start: `Rīga, Latvija -> 56.9493977,24.1051846`
- resolved finish: `Sigulda, Siguldas novads, LV-2150, Latvija -> 57.1540561,24.8567141`

Underlying CLI command:

```bash
target/release/ridi-router-cli generate-route \
  --tiles /home/toms/dev/ridi-router/map-data/output \
  --output-dir /home/toms/dev/ridi-router/map-data/routes \
  --format gpx \
  --rule-file /home/toms/dev/ridi-router/rule-examples/rules-default.json \
  start-finish \
  --start 56.9493977,24.1051846 \
  --finish 57.1540561,24.8567141
```

## Run status

- Hotpath profiling was active for **102.64 s** when I took the final snapshot.
- I then killed the run as requested.
- The route did **not** finish before the kill.
- No route output file was produced; `map-data/routes` did not exist after termination.

## Hotpath timing snapshot at kill time

Note: Hotpath timing totals are **inclusive cumulative time**, so totals above wall-clock time and percentages above 100% are expected.

| Function | Calls | Total | Why it matters |
|---|---:|---:|---|
| `graph::get_point_from_tiles` | 2,891,118 | **463.03 s** | biggest graph hydration / point lookup cost |
| `walker::move_forward_to_next_fork_with_context` | 51,338 | **373.48 s** | forward exploration is very expensive |
| `walker::get_fork_segments_for_segment_with_context` | 350,709 | **327.12 s** | heavy fork discovery / branching work |
| `navigator::generate_routes_with_context` | 3 | **281.61 s** | top-level route generation wrapper; search still deep in progress |
| `walker::get_roundabout_exits_with_context` | 9,970 | **269.03 s** | roundabout handling is a major hotspot here |
| `graph::get_line_from_tiles` | 2,005,209 | **239.06 s** | repeated line hydration/lookups add up heavily |
| `walker::move_backwards_to_prev_fork_with_context` | 15,883 | **156.14 s** | backward exploration is also expensive |
| `weights::weight_no_short_detours` | 22,086 | **61.33 s** | biggest route-logic weight in this run |
| `route::is_back_on_road_within_distance` | 8,227 | **53.77 s** | expensive backward-scan route check |
| `tile_manager::get_adjacent_by_id` | 3,263,938 | **51.04 s** | adjacency lookup stack is still significant |
| `weights::weight_heading` | 22,086 | **50.76 s** | another large route-logic cost |
| `graph::get_tag_set` | 510,716 | **49.66 s** | tag access is a meaningful nested cost |

Other notable weights still visible but secondary to graph/walker work:

- `weights::weight_rules_highway`: **38.22 s**
- `weights::weight_rules_surface`: **34.79 s**
- `weights::weight_rules_smoothness`: **32.80 s**
- `weights::weight_check_distance_to_next`: **15.85 s**

## Hotpath allocation snapshot at kill time

Note: this table is also **inclusive cumulative allocation bytes**, not peak resident memory.

| Function | Calls | Total alloc | Why it matters |
|---|---:|---:|---|
| `graph::get_point_from_tiles` | 2,891,151 | **57.8 GB** | largest cumulative allocator in the run |
| `tile_manager::get_rules_for_point` | 2,891,152 | **57.2 GB** | nearly as large; rule lookup churn is massive |
| `walker::get_fork_segments_for_segment_with_context` | 350,712 | **42.5 GB** | large allocation churn during branching |
| `walker::get_roundabout_exits_with_context` | 9,970 | **31.6 GB** | surprisingly large allocator for this route |
| `walker::move_forward_to_next_fork_with_context` | 51,338 | **30.5 GB** | major exploration churn |
| `navigator::generate_routes_with_context` | 3 | **23.1 GB** | route search still building substantial state |
| `walker::move_backwards_to_prev_fork_with_context` | 15,883 | **16.6 GB** | backward exploration is memory-heavy too |
| `weights::weight_heading` | 22,086 | **3.7 GB** | large route-logic allocator |
| `weights::weight_no_short_detours` | 22,086 | **2.6 GB** | meaningful allocator, but below walker/graph costs |
| `route::is_back_on_road_within_distance` | 8,227 | **2.3 GB** | same backward-scan pattern shows up in allocs |

Secondary but still visible allocation churn:

- `weights::weight_check_avoid_rules`: **1.2 GB**
- `weights::weight_no_sharp_turns`: **1.1 GB**
- `walker::move_to_roundabout_exit_with_context`: **1.0 GB**
- `weights::weight_check_distance_to_next`: **816.6 MB**
- `tile_manager::get_adjacent_by_id`: **590.8 MB**

## Thread snapshot near kill time

- RSS: **166.9 MB**
- thread count: **19**
- live alloc/dealloc diff: **6.7 MB**
- busiest worker threads were using roughly **12% to 40% CPU each** at the sample point
- the profiling helper thread `hp-functions` was also active while serving MCP queries

Top worker-thread cumulative allocation totals at the sample point:

- thread `1807948`: **19.9 GB**
- thread `1807945`: **11.9 GB**
- thread `1807943`: **11.8 GB**
- thread `1807949`: **11.5 GB**

## Main findings

1. The run really does stay busy well past 120 seconds worth of route-search work.
   - It was still actively exploring at the **102.64 s** Hotpath snapshot.
   - There was no finished route output before I killed it.

2. The dominant cost is still the same broad stack, not one isolated function.
   - **Graph hydration / lookup:** `get_point_from_tiles`, `get_line_from_tiles`
   - **Fork exploration:** `move_forward_to_next_fork_with_context`, `get_fork_segments_for_segment_with_context`, `get_roundabout_exits_with_context`
   - **Backward scanning:** `move_backwards_to_prev_fork_with_context`, `is_back_on_road_within_distance`
   - **Lookup plumbing:** `get_adjacent_by_id`, `get_point_by_id`, `get_tag_set`

3. Allocation churn is huge even though RSS stays modest.
   - Inclusive allocation totals are in the **tens of GB** for several functions.
   - `get_rules_for_point` remains a first-order allocation problem.
   - This looks more like repeated materialization/churn than a simple resident-memory blowup.

4. Route-logic weights matter, but they are not the top bottleneck.
   - The biggest route-logic timing costs are `weight_no_short_detours`, `weight_heading`, and the rules-based weights.
   - `weight_check_distance_to_next` is measurable, but it is not the headline issue on this run.

## Bottom line

For `riga,latvia -> sigulda,latvia`, the route search was still deep in graph/fork exploration after about **103 seconds** of Hotpath uptime. The biggest performance story is repeated graph hydration plus expensive fork/roundabout traversal, with very large cumulative allocation churn in `get_point_from_tiles` and `get_rules_for_point`. The run did not finish before being killed.
