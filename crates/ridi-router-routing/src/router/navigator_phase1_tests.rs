use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_fork::rusty_fork_test;

use crate::{
    router::{
        itinerary::Itinerary,
        navigator::{NavigationResult, WeightCalcResult},
        rules::RouterRules,
        weights::{WeightCalc, WeightCalcInput, WeightCalcStage},
    },
    test_utils::{test_dataset_1, RoutingTestContext},
};

use super::Navigator;

static ROUTE_GATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SECOND_ROUTE_GATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static PER_FORK_CALLS: AtomicUsize = AtomicUsize::new(0);

fn reset_counts() {
    ROUTE_GATE_CALLS.store(0, Ordering::SeqCst);
    SECOND_ROUTE_GATE_CALLS.store(0, Ordering::SeqCst);
    PER_FORK_CALLS.store(0, Ordering::SeqCst);
}

fn route_gate_last_segment_do_not_use(_input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    ROUTE_GATE_CALLS.fetch_add(1, Ordering::SeqCst);
    WeightCalcResult::LastSegmentDoNotUse()
}

fn second_route_gate_last_segment_do_not_use(_input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    SECOND_ROUTE_GATE_CALLS.fetch_add(1, Ordering::SeqCst);
    WeightCalcResult::LastSegmentDoNotUse()
}

fn route_gate_pass(_input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    ROUTE_GATE_CALLS.fetch_add(1, Ordering::SeqCst);
    WeightCalcResult::ForkChoiceUseWithWeight(0)
}

fn per_fork_counter(input: WeightCalcInput<'_, '_>) -> WeightCalcResult {
    PER_FORK_CALLS.fetch_add(1, Ordering::SeqCst);
    if input
        .ctx
        .point(input.current_fork_segment.get_end_point())
        .id
        == 5
    {
        return WeightCalcResult::ForkChoiceUseWithWeight(10);
    }
    WeightCalcResult::ForkChoiceUseWithWeight(1)
}

rusty_fork_test! {
    #![rusty_fork(timeout_ms = 2000)]

    #[test]
    fn route_level_last_segment_gate_runs_once_and_skips_per_fork_weights() {
        reset_counts();

        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();
        let itinerary = Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
        let navigator = Navigator::new(
            itinerary,
            RouterRules::default(),
            vec![
                WeightCalc {
                    calc: route_gate_last_segment_do_not_use,
                    name: "route_gate_last_segment_do_not_use".to_string(),
                    stage: WeightCalcStage::RouteOnce,
                },
                WeightCalc {
                    calc: per_fork_counter,
                    name: "per_fork_counter".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
            ],
            false,
        );

        assert!(matches!(navigator.generate_routes_with_context(&ctx), NavigationResult::Stuck));
        assert_eq!(ROUTE_GATE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(PER_FORK_CALLS.load(Ordering::SeqCst), 0);
    }
}

rusty_fork_test! {
    #![rusty_fork(timeout_ms = 2000)]

    #[test]
    fn multiple_route_level_gates_short_circuit_before_later_route_gates() {
        reset_counts();

        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();
        let itinerary = Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(7), Vec::new(), 0.0);
        let navigator = Navigator::new(
            itinerary,
            RouterRules::default(),
            vec![
                WeightCalc {
                    calc: route_gate_last_segment_do_not_use,
                    name: "route_gate_last_segment_do_not_use".to_string(),
                    stage: WeightCalcStage::RouteOnce,
                },
                WeightCalc {
                    calc: second_route_gate_last_segment_do_not_use,
                    name: "second_route_gate_last_segment_do_not_use".to_string(),
                    stage: WeightCalcStage::RouteOnce,
                },
            ],
            false,
        );

        assert!(matches!(navigator.generate_routes_with_context(&ctx), NavigationResult::Stuck));
        assert_eq!(ROUTE_GATE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(SECOND_ROUTE_GATE_CALLS.load(Ordering::SeqCst), 0);
    }
}

rusty_fork_test! {
    #![rusty_fork(timeout_ms = 2000)]

    #[test]
    fn passing_route_level_weights_run_once_per_fork_event() {
        reset_counts();

        let test_ctx = RoutingTestContext::new(test_dataset_1());
        let ctx = test_ctx.resolver();
        let itinerary = Itinerary::new_start_finish(test_ctx.point(1), test_ctx.point(5), Vec::new(), 0.0);
        let navigator = Navigator::new(
            itinerary,
            RouterRules::default(),
            vec![
                WeightCalc {
                    calc: route_gate_pass,
                    name: "route_gate_pass".to_string(),
                    stage: WeightCalcStage::RouteOnce,
                },
                WeightCalc {
                    calc: per_fork_counter,
                    name: "per_fork_counter".to_string(),
                    stage: WeightCalcStage::PerForkChoice,
                },
            ],
            false,
        );

        assert!(matches!(
            navigator.generate_routes_with_context(&ctx),
            NavigationResult::Finished(_)
        ));
        assert_eq!(ROUTE_GATE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(PER_FORK_CALLS.load(Ordering::SeqCst), 3);
    }
}
