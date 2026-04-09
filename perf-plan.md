# Performance implementation plan: `walker::move_forward_to_next_fork_with_context`

## Scope

Target hotspot from `./perf.md`:

- `walker::move_forward_to_next_fork_with_context`
  - 357,872 calls
  - 13.70 s total
  - 27.09% of measured time
  - 940.0 MB cumulative alloc
  - about **38.3 µs/call**
  - about **2.7 KB alloc/call**

Related hotspots in the same path:

- `walker::get_fork_segments_for_segment_with_context`: 3,835,443 calls, 6.23 s, 598.4 MB
- `weight_heading`: 141,086 calls, 2.86 s
- `get_roundabout_exits_with_context`: visible and expensive, but **excluded from this refactor pass**

This plan is now updated to reflect the implementation decisions we agreed on.

---

## Agreed decisions

1. **Do the larger refactor**, not just a tiny local optimization.
   - But first, add tests that cement current behavior where that behavior is worth preserving.
   - Then refactor.
   - Then re-run tests and perf checks.

2. **Create a specialized look-ahead helper for `weight_heading`.**
   - The helper must share internals with the main walker.
   - No duplicated rule logic.

3. **Exclude roundabouts from this implementation pass.**
   - Do not redesign roundabout traversal in this patch series.
   - Preserve current roundabout behavior.
   - Keep the new internals compatible with future roundabout work.

4. **Update `RoutingContext` with `with_...` accessors** for hot-path non-cloning access.

5. Optimize for **both runtime and allocations**.

6. Maintain **strict behavior preservation**.
   - Same routing behavior
   - Same fork semantics
   - Same error behavior where applicable

7. The heading look-ahead helper should be **specialized**, not a new fully general speculative walker.

---

## Current behavior we are preserving

Source focus:

- `crates/ridi-router-routing/src/router/walker.rs`
- `crates/ridi-router-routing/src/router/weights.rs`
- `crates/ridi-router-routing/src/routing_context.rs`

`move_forward_to_next_fork_with_context` currently:

1. walks forward from the current point
2. stops on finish, fork, dead end, or junction loop detection
3. uses these helpers to discover legal next steps:
   - `get_segments_for_point_with_context`
   - `get_fork_segments_for_segment_with_context`
   - `get_roundabout_exits_with_context`
4. validates explicit fork choices
5. updates the walked route as it advances

`weight_heading` currently:

1. creates a fresh `Walker`
2. calls `move_forward_to_next_fork_with_context`
3. uses the resulting segment orientation to estimate heading toward the next itinerary point

The refactor should preserve those outcomes, while changing how the internals compute them.

---

## Main problems to address

## 1. Common-path allocation churn in the walker

Today the main walker loop asks helpers to fully build `SegmentList` values even in the common case where there are:

- no legal next segments
- exactly one legal next segment
- or one chosen next segment out of multiple

That means the hot corridor-following case still pays for:

- adjacency clones
- temporary segment collections
- repeated scans through collected choices

This is a direct match for the allocation profile.

## 2. `RoutingContext::point()` and `RoutingContext::adjacent()` clone owned data

Current hot-path behavior:

- `point()` clones `MapDataPoint`
- `adjacent()` clones cached adjacency vectors

That means even cache hits still allocate or copy owned containers.

## 3. `get_fork_segments_for_segment_with_context` recomputes filtering work

Per call it currently:

- loads point data
- scans rules
- builds temporary rule lists
- scans adjacency
- creates fresh segments

This is individually cheap, but it runs **millions of times**.

## 4. `weight_heading` is using the full walker for a small question

It only needs a specialized look-ahead answer, but currently pays for:

- a fresh `Walker`
- route mutation machinery
- loop detector updates
- full fork result behavior

That is too heavy for a scoring probe.

---

## Implementation strategy

We will do a larger internal refactor, but in a controlled order:

1. **Add/expand tests first** for preserved behavior.
2. **Introduce shared low-level next-step classification internals.**
3. **Refactor the main walker to use them.**
4. **Add specialized heading look-ahead on top of the same shared internals.**
5. **Update `RoutingContext` with `with_...` accessors** so the hot path can inspect cached data without cloning.
6. Re-run tests and perf measurements.

Roundabout optimization is explicitly out of scope for this pass.

---

## Detailed plan

## Phase 0: Cement current behavior with tests

Before touching the main logic, add or strengthen tests around non-roundabout behavior.

### Test goals

We want tests that preserve externally visible semantics, not tests that freeze internal implementation details.

### Add or strengthen tests for

#### Keep and re-run existing coverage

These existing tests already protect part of the current behavior and should stay green throughout the refactor:

- `walker_same_start_end`
- `walker_error_on_wrong_choice`
- `walker_choose_path`
- `walker_reach_dead_end_walk_back`
- `handle_roundabout`
- `follow_one_way`
- existing `rule_test(...)` coverage in `walker.rs`
- existing `weights_phase1_tests.rs` coverage that touches routing/weight behavior

#### Add concrete new tests

##### In `crates/ridi-router-routing/src/router/walker.rs`

1. `walker_moves_through_single_choice_corridor_until_finish`
   - start on a non-roundabout corridor
   - no explicit fork choice
   - expect `Finish`
   - assert route segments match the corridor exactly

2. `walker_returns_fork_after_single_choice_corridor`
   - start before a corridor that leads into a fork
   - expect the walker to consume the corridor and return `Fork(...)` only at the first real multi-choice point
   - assert fork choice count and end-point IDs

3. `walker_returns_dead_end_after_single_choice_corridor`
   - start before a corridor that leads into a dead end
   - expect `DeadEnd` after walking the corridor
   - assert route segments match the traversed corridor

4. `walker_follows_explicit_choice_after_corridor`
   - start before a corridor, set a valid explicit fork choice for the fork reached after the corridor
   - expect the walker to consume the corridor and then follow the chosen branch
   - assert chosen branch is taken and route history is correct

5. `walker_wrong_choice_after_corridor_returns_same_available_points`
   - same setup as above, but choose an invalid point
   - expect `WalkerError::WrongForkChoice`
   - assert the returned `available_fork_ids` match the currently legal choices

6. `walker_filters_reverse_edge_from_incoming_segment`
   - non-roundabout fork with a possible reverse edge back to the previous point
   - assert the reverse edge is not offered as a legal continuation

7. `walker_respects_one_way_after_corridor`
   - non-roundabout corridor leading into a one-way restricted choice
   - assert the wrong-way branch is excluded from `Fork(...)` choices

8. `walker_respects_only_allowed_rule_after_corridor`
   - build a point with `OnlyAllowed` rules for the incoming line
   - assert only permitted outgoing lines are surfaced

9. `walker_respects_not_allowed_rule_after_corridor`
   - build a point with `NotAllowed` rules for the incoming line
   - assert forbidden outgoing lines are excluded

10. `walker_start_point_rule_filtering_matches_current_behavior`
   - add a start-point-focused case that exercises `get_segments_for_point_with_context` semantics
   - assert legal choices from the initial point match current behavior

11. `move_backwards_to_prev_fork_still_returns_expected_choices_after_refactor`
   - explicit regression test for non-roundabout backtracking
   - expect the previous fork and available choices to remain unchanged

##### In `crates/ridi-router-routing/src/router/weights_phase1_tests.rs` or a dedicated heading-look-ahead test file

12. `heading_look_ahead_returns_finish_when_candidate_reaches_finish_before_next_fork`
   - candidate branch reaches finish without another decision point
   - expect specialized helper result `Finish`-equivalent behavior

13. `heading_look_ahead_returns_dead_end_when_candidate_dies_before_next_fork`
   - candidate branch ends before another decision point
   - expect `DeadEnd`-equivalent behavior

14. `heading_look_ahead_returns_immediate_decision_when_candidate_endpoint_is_already_a_fork`
   - candidate endpoint itself is already a decision point
   - assert the helper returns the same effective meaning that current `weight_heading` relies on

15. `heading_look_ahead_returns_approach_segment_after_single_choice_corridor`
   - candidate branch goes through exactly one corridor before the next decision point
   - assert the returned approach segment is the last traversed segment before the fork

16. `weight_heading_matches_current_result_on_non_roundabout_corridor_case`
   - add a fixed fixture where current `weight_heading` behavior is easy to verify
   - use it as a regression test before and after the helper swap

17. `weight_heading_matches_current_result_on_immediate_fork_case`
   - candidate point is already at a decision point
   - assert fallback behavior remains identical

##### In `crates/ridi-router-routing/src/routing_context.rs`

18. `with_point_exposes_same_visible_data_as_point`
   - for the same point ref, compare the fields observed through `with_point(...)` with the current `point()` result
   - this is a behavior-equivalence test, not a perf test

19. `with_adjacent_exposes_same_visible_data_as_adjacent`
   - for the same point ref, compare the adjacency seen through `with_adjacent(...)` with the current `adjacent()` result
   - order and contents should match current behavior

20. `with_point_and_with_adjacent_work_with_cached_data`
   - touch data once, then access again through the `with_...` APIs
   - assert returned visible results stay correct when served from cache

#### Test intent

These tests are meant to lock down externally visible semantics while still leaving room for internal restructuring:

- same route outcomes
- same fork surfacing points
- same rule filtering results
- same explicit-choice validation behavior
- same heading-scoring inputs in non-roundabout cases
### Important constraint

Do **not** broaden behavior in this phase.

This is about preserving current semantics so the refactor has a clear safety net.

---

## Phase 1: Introduce shared low-level traversal classification

### Goal

Create one internal classification layer that both the main walker and the new heading look-ahead helper will use.

### Proposed shape

Introduce an internal result roughly like this:

```rust
enum NextStepClass {
    None,
    One(Segment),
    Many(SmallVec<[Segment; 4]>),
}
```

This does **not** have to be the final exact type name, but the core idea is:

- avoid full `SegmentList` allocation on the 0/1 cases
- surface multiple choices only when multiple choices really exist

### Shared classifier responsibilities

For the non-roundabout path, shared internals should handle:

- start-point segment discovery
- incoming-segment continuation discovery
- reverse-edge exclusion
- one-way exclusion
- `OnlyAllowed` / `NotAllowed` rule filtering
- explicit fork-choice validation support

### Explicit design rule

The low-level classifier must be the **single source of truth** for legal next-step computation in this refactor.

That prevents semantic drift between:

- the main walker
- the heading look-ahead helper

---

## Phase 2: Refactor the main walker to use the classifier

### Goal

Make `move_forward_to_next_fork_with_context` cheap on the common path.

### Target behavior

The main loop should:

- return `Finish` immediately on finish
- return `DeadEnd` immediately on no legal next step
- advance immediately on exactly one legal next step
- return `Fork(...)` only when multiple legal choices actually exist and no explicit choice is set
- validate explicit choices without paying unnecessary allocation cost on the success path

### Main fast-path idea

The common corridor case should avoid:

- building `SegmentList`
- collecting all choices just to grab the first
- scanning temporary collections multiple times

### Error-path policy

For explicit wrong fork choices, it is acceptable to do some additional work to build the `available_fork_ids` payload.

That is an error path, not the hot path.

### Strict preservation requirement

This refactor should preserve:

- `Fork` timing behavior from the caller’s point of view
- `WrongForkChoice` shape and meaning
- finish/dead-end behavior
- junction-loop behavior in non-roundabout paths

---

## Phase 3: Add the specialized heading look-ahead helper

### Goal

Replace `weight_heading`’s full walker call with a narrow helper that answers only what heading scoring needs.

### What the helper should do

Given a candidate fork segment, it should follow the same shared next-step logic until it reaches one of these non-roundabout outcomes:

- `Finish`
- `DeadEnd`
- next decision point

### What it should return

A specialized answer such as:

```rust
enum HeadingLookAheadResult {
    DeadEnd,
    Finish,
    Decision {
        approach_segment: Option<Segment>,
    },
}
```

Exact naming may differ, but the helper should return only the information `weight_heading` needs.

### What it should not do

It should **not**:

- build a full `Route`
- mutate a full `Walker`
- update loop detector route history
- expose a general speculative traversal API

### Shared-internals requirement

This helper must use the same low-level next-step classification as the main walker.

That is the key correctness rule for this part of the refactor.

### Scope limit

This helper should be specialized for **non-roundabout** heading look-ahead in this pass.

If roundabout behavior is encountered, we should preserve current behavior conservatively rather than invent new semantics here.

---

## Phase 4: Add `RoutingContext` `with_...` accessors for hot paths

### Goal

Let hot traversal code inspect cached structures without cloning owned data.

### Agreed direction

Add `with_...` style accessors.

Examples of the intended pattern:

```rust
ctx.with_point(point_ref, |point| {
    // inspect point.rules, flags, etc.
});

ctx.with_adjacent(point_ref, |adjacent| {
    // inspect adjacency by reference
});
```

Exact naming can vary, but the intent is:

- no cloning of `MapDataPoint` in the hot path when read-only inspection is enough
- no cloning of adjacency vectors in the hot path when read-only iteration is enough

### Where to use them first

Use these accessors first in the traversal hotspot path:

- walker fast-path helpers
- heading look-ahead helper

Do not try to convert the whole codebase at once.

### Behavior rule

These are API-level internal changes only.

No behavioral change should result from introducing them.

---

## Phase 5: Measure both runtime and allocations

This refactor is successful only if it improves both:

- runtime
- cumulative allocations

### Main metrics to compare

1. `walker::move_forward_to_next_fork_with_context`
   - total time
   - cumulative alloc
   - average alloc per call

2. `walker::get_fork_segments_for_segment_with_context`
   - total time
   - cumulative alloc
   - average alloc per call

3. `weight_heading`
   - total time
   - evidence that the new helper is cheaper than full walker traversal

4. route-generation wall time for the same scenario in `./perf.md`

### Expected profile shape after improvement

We should ideally see:

- lower alloc total in the walker path first
- lower per-call cost in fork classification helpers
- lower `weight_heading` cost from the specialized look-ahead helper
- reduced time in `move_forward_to_next_fork_with_context`

---

## Out of scope for this pass

These items are intentionally excluded from this implementation round:

1. **Roundabout redesign**
   - no refactor of `get_roundabout_exits_with_context`
   - no refactor of `move_to_roundabout_exit_with_context`
   - no attempt to optimize roundabout allocations yet

2. **Broad route-policy changes**
   - no change in scoring philosophy
   - no change in route selection semantics

3. **General speculative traversal framework**
   - the new heading helper should stay narrow and specialized

4. **Whole-program transition caching**
   - promising, but not required for this pass

---

## Risk management

## Main risks

1. semantic drift between main walker and heading look-ahead helper
2. accidental change to fork or dead-end behavior
3. accidental change to rule filtering behavior
4. accidental roundabout regression from touching shared code too broadly

## Mitigations

1. tests first
2. one shared non-roundabout next-step classifier
3. strict behavior preservation as a design rule
4. keep roundabout-specific code out of the first optimized path where possible
5. re-run tests after each phase, not only at the end

---

## Recommended patch order

### Patch 1
Add and strengthen tests for current non-roundabout behavior.

### Patch 2
Add `RoutingContext` `with_...` accessors for point/adjacent read access.

### Patch 3
Introduce shared non-roundabout next-step classification internals.

### Patch 4
Refactor `move_forward_to_next_fork_with_context` to use the shared classifier fast path.

### Patch 5
Add specialized heading look-ahead helper built on the same classifier.

### Patch 6
Refactor `weight_heading` to use the new helper.

### Patch 7
Run full tests and refresh the perf measurements.

This order keeps correctness visible and reviewable.

---

## Bottom line

The implementation direction is now:

- **larger refactor**
- **tests first**
- **strict behavior preservation**
- **shared internals between walker and heading look-ahead**
- **non-cloning `RoutingContext` hot-path access via `with_...` APIs**
- **focus on both runtime and allocation reduction**
- **exclude roundabout optimization from this pass**

The highest-value first target remains the same:

1. make the walker cheap on the 0/1-choice path
2. stop cloning point and adjacency data in the hot path
3. stop using the full walker for `weight_heading` look-ahead
