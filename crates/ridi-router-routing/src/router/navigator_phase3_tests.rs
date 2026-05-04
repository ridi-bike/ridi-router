use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_fork::rusty_fork_test;

use crate::{
    router::{
        itinerary::Itinerary,
        navigator::{NavigationResult, WeightCalcResult},
        rules::RouterRules,
        weights::{WeightCalc, WeightCalcInput, WeightCalcStage},
    },
    test_utils::{route_matches_ids, test_dataset_1, RoutingTestContext},
};

use super::Navigator;

static CHEAP_REJECTED_CHOICE_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXPENSIVE_REJECTED_CHOICE_CALLS: AtomicUsize = AtomicUsize::new(0);
static EXPENSIVE_ACCEPTED_CHOICE_CALLS: AtomicUsize = AtomicUsize::new(0);
static DISCARDED_FORK_EXPENSIVE_CALLS: AtomicUsize = AtomicUsize::new(0);

fn reset_counts() {
    CHEAP_REJECTED_CHOICE_CALLS.store(0, Ordering::SeqCst);
    EXPENSIVE_REJECTED_CHOICE_CALLS.store(0, Ordering::SeqCst);
    EXPENSIVE_ACCEPTED_CHOICE_CALLS.store(0, Ordering::SeqCst);
    DISCARDED_FORK_EXPENSIVE_CALLS.store(0, Ordering::SeqCst);
}

fn cheap_reject_one_choice(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    if input
        .ctx
        .point(input.current_fork_segment.get_end_point())
        .id
        == 5
    {
        CHEAP_REJECTED_CHOICE_CALLS.fetch_add(1, Ordering::SeqCst);
        return WeightCalcResult::ForkChoiceDoNotUse();
    }

    WeightCalcResult::ForkChoiceUseWithWeight(1)
}

fn expensive_prefer_six(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    if input
        .ctx
        .point(input.current_fork_segment.get_end_point())
        .id
        == 5
    {
        EXPENSIVE_REJECTED_CHOICE_CALLS.fetch_add(1, Ordering::SeqCst);
    } else {
        EXPENSIVE_ACCEPTED_CHOICE_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    if input
        .ctx
        .point(input.current_fork_segment.get_end_point())
        .id
        == 6
    {
        return WeightCalcResult::ForkChoiceUseWithWeight(10);
    }

    WeightCalcResult::ForkChoiceUseWithWeight(1)
}

fn discard_fork_immediately(_input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    WeightCalcResult::LastSegmentDoNotUse()
}

fn expensive_after_discard(_input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    DISCARDED_FORK_EXPENSIVE_CALLS.fetch_add(1, Ordering::SeqCst);
    WeightCalcResult::ForkChoiceUseWithWeight(1)
}

rusty_fork_test! {
    #![rusty_fork(timeout_ms = 2000)]

    #[test]
    fn per_fork_do_not_use_short_circuits_later_weight_calcs_for_that_choice() {
        reset_counts();

        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();
        let itinerary = Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
        let navigator = Navigator::new(
            itinerary,
            RouterRules::default(),
            vec![
                WeightCalc {
                    calc: cheap_reject_one_choice,
                    name: "cheap_reject_one_choice".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
                WeightCalc {
                    calc: expensive_prefer_six,
                    name: "expensive_prefer_six".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
            ],
            false,
        );

        let route = match navigator.generate_routes_with_context(&ctx) {
            NavigationResult::Finished(route) => route,
            _ => panic!("expected finished route"),
        };

        assert!(route_matches_ids(&ctx, route, &[2, 3, 6, 7]));
        assert!(CHEAP_REJECTED_CHOICE_CALLS.load(Ordering::SeqCst) > 0);
        assert_eq!(EXPENSIVE_REJECTED_CHOICE_CALLS.load(Ordering::SeqCst), 0);
        assert!(EXPENSIVE_ACCEPTED_CHOICE_CALLS.load(Ordering::SeqCst) > 0);
    }
}

rusty_fork_test! {
    #![rusty_fork(timeout_ms = 2000)]

    #[test]
    fn last_segment_do_not_use_short_circuits_remaining_per_fork_weight_calcs() {
        reset_counts();

        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();
        let itinerary = Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
        let navigator = Navigator::new(
            itinerary,
            RouterRules::default(),
            vec![
                WeightCalc {
                    calc: discard_fork_immediately,
                    name: "discard_fork_immediately".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
                WeightCalc {
                    calc: expensive_after_discard,
                    name: "expensive_after_discard".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
            ],
            false,
        );

        assert!(matches!(navigator.generate_routes_with_context(&ctx), NavigationResult::Stuck));
        assert_eq!(DISCARDED_FORK_EXPENSIVE_CALLS.load(Ordering::SeqCst), 0);
    }
}
