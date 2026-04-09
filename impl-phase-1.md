# Implementation Phase 1: Lock down current behavior with tests

## Goal
Create a safety net before any hot-path refactor. This phase exists to preserve external behavior while later phases change internals.

## Why this is a separate phase
The performance plan explicitly favors a larger refactor, but only after current semantics are pinned down. This phase reduces the main project risk: semantic drift.

## Rollout note
These phases are for implementation order and logical separation inside a single PR.
Do not treat them as separate PR boundaries.

## In scope
- Keep all relevant existing tests green
- Add or strengthen non-roundabout behavior tests in:
  - `crates/ridi-router-routing/src/router/walker.rs`
  - `crates/ridi-router-routing/src/router/weights_phase1_tests.rs` or a dedicated heading-look-ahead test file
  - `crates/ridi-router-routing/src/routing_context.rs`
- Add test fixtures/helpers only when needed to express behavior clearly
- Document any current behavior that looks odd but is intentionally preserved

## Out of scope
- No production refactor yet
- No roundabout redesign
- No performance-driven behavior changes

## Required test coverage
### Keep existing coverage green
- `walker_same_start_end`
- `walker_error_on_wrong_choice`
- `walker_choose_path`
- `walker_reach_dead_end_walk_back`
- `handle_roundabout`
- `follow_one_way`
- existing `rule_test(...)` coverage in `walker.rs`
- existing `weights_phase1_tests.rs` coverage that touches routing/weight behavior

### Add or strengthen walker tests
1. `walker_moves_through_single_choice_corridor_until_finish`
2. `walker_returns_fork_after_single_choice_corridor`
3. `walker_returns_dead_end_after_single_choice_corridor`
4. `walker_follows_explicit_choice_after_corridor`
5. `walker_wrong_choice_after_corridor_returns_same_available_points`
6. `walker_filters_reverse_edge_from_incoming_segment`
7. `walker_respects_one_way_after_corridor`
8. `walker_respects_only_allowed_rule_after_corridor`
9. `walker_respects_not_allowed_rule_after_corridor`
10. `walker_start_point_rule_filtering_matches_current_behavior`
11. `move_backwards_to_prev_fork_still_returns_expected_choices_after_refactor`

### Add heading / look-ahead regression tests
12. `heading_look_ahead_returns_finish_when_candidate_reaches_finish_before_next_fork`
13. `heading_look_ahead_returns_dead_end_when_candidate_dies_before_next_fork`
14. `heading_look_ahead_returns_immediate_decision_when_candidate_endpoint_is_already_a_fork`
15. `heading_look_ahead_returns_approach_segment_after_single_choice_corridor`
16. `weight_heading_matches_current_result_on_non_roundabout_corridor_case`
17. `weight_heading_matches_current_result_on_immediate_fork_case`

### Add `RoutingContext` equivalence tests
18. `with_point_exposes_same_visible_data_as_point`
19. `with_adjacent_exposes_same_visible_data_as_adjacent`
20. `with_point_and_with_adjacent_work_with_cached_data`

## Implementation notes
- Test visible outcomes, not internal helper shapes
- Prefer concise fixtures that make corridor/fork/dead-end behavior obvious
- Preserve roundabout coverage, but do not expand roundabout behavior in this phase
- If an existing behavior seems inefficient or surprising, freeze it now and revisit only in a later dedicated change

## Deliverables
- New or updated tests committed and passing
- Any small test helpers needed to keep fixtures readable
- A short note in the single PR description listing preserved behaviors the new tests now protect

## Validation
Recommended checks:
- `cargo test -p ridi-router-routing`
- Targeted reruns for walker/weights/routing_context tests while iterating

## Exit criteria
- All existing relevant tests still pass
- All planned non-roundabout regression tests are added, or any omissions are explicitly justified
- No production behavior changes are introduced in this phase

## Handoff to next phase
Once this phase is complete, later refactors can safely change internals as long as these tests remain green.
