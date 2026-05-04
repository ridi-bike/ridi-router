use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    map_data::graph::MapDataPointRef,
    router::{route::segment::Segment, rules::RouterRules, weights::WeightCalcStage},
    routing_api::{Coords, RouteMode},
    RoutingContext,
};

pub trait RoutingProgressSink: Send + Sync {
    fn emit(&self, envelope: RoutingProgressEnvelope);
}

pub type RoutingProgressListener = Arc<dyn RoutingProgressSink>;

#[derive(Clone)]
pub(crate) struct RoutingProgressReporter {
    listener: Option<RoutingProgressListener>,
}

impl RoutingProgressReporter {
    #[allow(dead_code)]
    pub(crate) fn disabled() -> Self {
        Self { listener: None }
    }

    pub(crate) fn new(listener: Option<RoutingProgressListener>) -> Self {
        Self { listener }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.listener.is_some()
    }

    pub(crate) fn for_itinerary(&self, itinerary_id: u64) -> ItineraryProgressReporter {
        ItineraryProgressReporter {
            listener: self.listener.clone(),
            itinerary_id,
            sequence: 0,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn emit(&self, envelope: RoutingProgressEnvelope) {
        let Some(listener) = &self.listener else {
            return;
        };
        listener.emit(envelope);
    }
}

pub(crate) struct ItineraryProgressReporter {
    listener: Option<RoutingProgressListener>,
    itinerary_id: u64,
    sequence: u64,
}

impl ItineraryProgressReporter {
    #[allow(dead_code)]
    pub(crate) fn disabled(itinerary_id: u64) -> Self {
        Self {
            listener: None,
            itinerary_id,
            sequence: 0,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.listener.is_some()
    }

    pub(crate) fn emit(&mut self, event: RoutingProgressEvent) {
        let Some(listener) = &self.listener else {
            return;
        };
        self.sequence += 1;
        listener.emit(RoutingProgressEnvelope {
            itinerary_id: Some(self.itinerary_id),
            sequence: self.sequence,
            timestamp_ms: timestamp_ms(),
            event,
        });
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingProgressEnvelope {
    pub itinerary_id: Option<u64>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub event: RoutingProgressEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RoutingProgressEvent {
    RoutingRequestMetadata {
        mode: RouteMode,
        rules: Box<RouterRules>,
    },
    SnappingMetadata {
        start: Box<SnapPointMetadata>,
        finish: Box<SnapPointMetadata>,
    },
    ItineraryMetadata {
        itinerary_id: u64,
        generation_pass: GenerationPassMetadata,
        start: MapPointSnapshot,
        finish: MapPointSnapshot,
        waypoints: Vec<MapPointSnapshot>,
        waypoint_selection: WaypointSelectionMetadata,
    },
    SegmentAdvanced {
        segment: MapSegmentSnapshot,
    },
    ForkReached {
        at: MapPointSnapshot,
        choices: Vec<MapSegmentSnapshot>,
    },
    ForkChoicesFiltered {
        at: MapPointSnapshot,
        available: Vec<MapSegmentSnapshot>,
        previously_discarded: Vec<MapPointSnapshot>,
        remaining: Vec<MapSegmentSnapshot>,
    },
    ForkRouteRuleEvaluated {
        at: MapPointSnapshot,
        evaluation: WeightEvaluation,
    },
    ForkChoiceRuleEvaluated {
        at: MapPointSnapshot,
        choice: MapSegmentSnapshot,
        evaluation: WeightEvaluation,
    },
    ForkChoiceEvaluated {
        at: MapPointSnapshot,
        choice: MapSegmentSnapshot,
        accepted: bool,
        total_weight: u32,
        evaluations: Vec<WeightEvaluation>,
        rejection: Option<ForkChoiceRejection>,
    },
    ForkDecisionMade {
        at: MapPointSnapshot,
        selected: Option<MapSegmentSnapshot>,
        choices: Vec<ForkChoiceSummary>,
        reason: ForkDecisionReason,
    },
    DeadEndReached {
        at: MapPointSnapshot,
    },
    SegmentBacktracked {
        segment: MapSegmentSnapshot,
        reason: BacktrackReason,
    },
    ItineraryFinished {
        status: ItineraryStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapPointRole {
    Start,
    Finish,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapPointMetadata {
    pub role: SnapPointRole,
    pub requested: Coords,
    pub snapped: MapPointSnapshot,
    pub preferred_highway_filter: Option<Vec<String>>,
    pub preferred_highway_match_found: bool,
    pub avoid_residential: bool,
    pub rules_used: RouterRules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationPassMetadata {
    pub avoid_residential: bool,
    pub round_trip_bearing_adjustment: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaypointSelectionMetadata {
    DirectStartFinish,
    StartFinishGenerated {
        from_waypoint_requested: Coords,
        from_waypoint_snapped: MapPointSnapshot,
        to_waypoint_requested: Coords,
        to_waypoint_snapped: MapPointSnapshot,
        bearing_from_start_to_finish: f32,
        bearing_from_finish_to_start: f32,
        distance_m: f32,
    },
    RoundTripGenerated {
        bearing: f32,
        adjusted_bearing: f32,
        target_distance_m: u32,
        side_left_requested: Coords,
        side_left_snapped: MapPointSnapshot,
        tip_requested: Coords,
        tip_snapped: MapPointSnapshot,
        side_right_requested: Coords,
        side_right_snapped: MapPointSnapshot,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPointSnapshot {
    pub tile_col: u32,
    pub tile_row: u32,
    pub point_id: u64,
    pub lat: f32,
    pub lon: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSegmentSnapshot {
    pub line_tile_col: u32,
    pub line_tile_row: u32,
    pub line_id: u64,
    pub from: MapPointSnapshot,
    pub to: MapPointSnapshot,
    pub highway: Option<String>,
    pub name: Option<String>,
    pub hw_ref: Option<String>,
    pub surface: Option<String>,
    pub smoothness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightEvaluation {
    pub name: String,
    pub stage: WeightCalcStageSnapshot,
    pub result: WeightEvaluationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeightCalcStageSnapshot {
    RouteOnce,
    PerForkChoice,
}

impl From<WeightCalcStage> for WeightCalcStageSnapshot {
    fn from(value: WeightCalcStage) -> Self {
        match value {
            WeightCalcStage::RouteOnce => Self::RouteOnce,
            WeightCalcStage::PerForkChoice => Self::PerForkChoice,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeightEvaluationResult {
    Accepted { weight: u8, reason: Option<String> },
    RejectedChoice { reason: Option<String> },
    RejectedRoute { reason: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkChoiceSummary {
    pub choice: MapSegmentSnapshot,
    pub accepted: bool,
    pub total_weight: u32,
    pub evaluations: Vec<WeightEvaluation>,
    pub rejection: Option<ForkChoiceRejection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForkChoiceRejection {
    PreviouslyDiscarded,
    RejectedByRule { rule_name: String },
    RouteRejectedByRule { rule_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForkDecisionReason {
    SelectedHighestWeight { selected_weight: u32 },
    AllChoicesPreviouslyDiscarded,
    AllChoicesRejected,
    RouteRejectedByRule { rule_name: String },
    NoChoicesAvailable,
    StuckAtStart,
    Backtracking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BacktrackReason {
    DeadEnd,
    NoAcceptedForkChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItineraryStatus {
    Finished,
    Stuck,
    Stopped,
}

#[derive(Debug, Clone)]
pub(crate) struct ItineraryMetadataPayload {
    pub itinerary_id: u64,
    pub generation_pass: GenerationPassMetadata,
    pub start: MapPointSnapshot,
    pub finish: MapPointSnapshot,
    pub waypoints: Vec<MapPointSnapshot>,
    pub waypoint_selection: WaypointSelectionMetadata,
}

impl ItineraryMetadataPayload {
    pub(crate) fn into_event(self) -> RoutingProgressEvent {
        RoutingProgressEvent::ItineraryMetadata {
            itinerary_id: self.itinerary_id,
            generation_pass: self.generation_pass,
            start: self.start,
            finish: self.finish,
            waypoints: self.waypoints,
            waypoint_selection: self.waypoint_selection,
        }
    }
}

pub(crate) fn point_snapshot(
    ctx: &RoutingContext<'_>,
    point_ref: &MapDataPointRef,
) -> MapPointSnapshot {
    let tile_id = point_ref.get_tile_id();
    let (lat, lon) = ctx.point_coords(point_ref);
    MapPointSnapshot {
        tile_col: tile_id.col as u32,
        tile_row: tile_id.row as u32,
        point_id: ctx.point_id(point_ref),
        lat,
        lon,
    }
}

pub(crate) fn segment_snapshot_from(
    ctx: &RoutingContext<'_>,
    from_point: &MapDataPointRef,
    segment: &Segment,
) -> MapSegmentSnapshot {
    let line_ref = segment.get_line();
    let tile_id = line_ref.get_tile_id();
    let line = ctx.line(line_ref);
    let tags = ctx.tag_set(&line.tags);
    MapSegmentSnapshot {
        line_tile_col: tile_id.col as u32,
        line_tile_row: tile_id.row as u32,
        line_id: line_ref.get_element_id(),
        from: point_snapshot(ctx, from_point),
        to: point_snapshot(ctx, segment.get_end_point()),
        highway: ctx.with_tag_value(&tags.highway, |value| value.map(ToString::to_string)),
        name: ctx.with_tag_value(&tags.name, |value| value.map(ToString::to_string)),
        hw_ref: ctx.with_tag_value(&tags.hw_ref, |value| value.map(ToString::to_string)),
        surface: ctx.with_tag_value(&tags.surface, |value| value.map(ToString::to_string)),
        smoothness: ctx.with_tag_value(&tags.smoothness, |value| value.map(ToString::to_string)),
    }
}

pub(crate) fn segment_snapshot_infer_from_line(
    ctx: &RoutingContext<'_>,
    segment: &Segment,
) -> MapSegmentSnapshot {
    let line = ctx.line(segment.get_line());
    let from = if &line.points.0 == segment.get_end_point() {
        &line.points.1
    } else {
        &line.points.0
    };
    segment_snapshot_from(ctx, from, segment)
}
