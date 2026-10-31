use macroquad::prelude::*;

use crate::map_rendering_spec::{CircleStyle, DottedFillStyle, FillStyle, LineStyle, TextStyle};
use crate::space::Space;
use crate::tile_address::TileAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub address: TileAddress,
    pub style_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleKey {
    pub zoom_range_index: usize,
    pub rule_index: usize,
    pub rule_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkGeneration(pub u64);

#[derive(Debug, Clone)]
pub enum WorkRequest {
    BuildTile {
        tile: TileAddress,
        style_revision: u64,
        generation: WorkGeneration,
    },
}

#[derive(Debug, Clone)]
pub enum WorkerMessage {
    CompletedPart {
        tile_key: TileKey,
        rule_key: RuleKey,
        generation: WorkGeneration,
        part: Box<RenderedTilePart>,
    },
    TileComplete {
        tile_key: TileKey,
        generation: WorkGeneration,
    },
    TileMissing {
        tile_key: TileKey,
        generation: WorkGeneration,
    },
    TileFailed {
        tile_key: TileKey,
        generation: WorkGeneration,
        error: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileBuildState {
    Missing,
    Queued,
    Building,
    Partial,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementPolicy {
    TileByTile,
    LayerByLayer,
    WholeViewportWhenReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileFailure {
    Missing,
    Failed,
}

pub trait TileRetryPolicy {
    fn should_retry(
        &self,
        tile: TileAddress,
        generation: WorkGeneration,
        failure: TileFailure,
    ) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EveryTenthGenerationRetryPolicy;

impl TileRetryPolicy for EveryTenthGenerationRetryPolicy {
    fn should_retry(
        &self,
        _tile: TileAddress,
        generation: WorkGeneration,
        _failure: TileFailure,
    ) -> bool {
        generation.0 != 0 && generation.0 % 10 == 0
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TileVertex {
    pub local: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct FillPatternUniforms {
    pub spacing_px: f32,
    pub radius_px: f32,
    pub dot_color: Color,
    pub base_color: Option<Color>,
}

impl FillPatternUniforms {
    pub fn from_style(style: &DottedFillStyle, base_color: Option<Color>) -> Self {
        Self {
            spacing_px: style.spacing_px,
            radius_px: style.radius_px,
            dot_color: style.color(),
            base_color,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FillMesh {
    pub rule_key: RuleKey,
    pub chunk_index: usize,
    pub vertices: Vec<TileVertex>,
    pub indices: Vec<u16>,
    pub color: Option<Color>,
    pub fill_pattern: Option<FillPatternUniforms>,
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub from: [f32; 2],
    pub to: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct LineBatch {
    pub color: Color,
    pub width_px: f32,
    pub dash: Option<Vec<f32>>,
    pub segments: Vec<LineSegment>,
}

#[derive(Debug, Clone)]
pub struct PointBatch {
    pub color: Color,
    pub radius_px: f32,
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone)]
pub struct LabelCandidate {
    pub local: [f32; 2],
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LabelBatch {
    pub color: Color,
    pub halo_color: Option<Color>,
    pub halo_width_px: Option<f32>,
    pub size_px: f32,
    pub offset_px: [f32; 2],
    pub labels: Vec<LabelCandidate>,
}

#[derive(Debug, Clone)]
pub enum RenderedPartKind {
    FillMesh(FillMesh),
    LineBatch(LineBatch),
    PointBatch(PointBatch),
    LabelBatch(LabelBatch),
}

#[derive(Debug, Clone)]
pub struct RenderedTilePart {
    pub rule_key: RuleKey,
    pub chunk_index: usize,
    pub layer_name: String,
    pub kind: RenderedPartKind,
    pub cpu_bytes: usize,
}

pub fn local_to_screen(space: &Space, address: TileAddress, local: [f32; 2]) -> Vec2 {
    let n = 2.0_f64.powi(address.z as i32);
    let mercator_x = (address.x as f64 + local[0] as f64) / n;
    let mercator_y = (address.y as f64 + local[1] as f64) / n;
    let screen =
        space.mercator_to_map_screen(mercator_x, mercator_y, screen_width(), screen_height());
    vec2(screen.x as f32, screen.y as f32)
}

pub fn line_batch_from_style(style: &LineStyle, casing: bool) -> (Color, f32, Option<Vec<f32>>) {
    if casing {
        (
            style.casing_color().unwrap_or_else(|| style.color()),
            style.width_px + style.casing_width_px.unwrap_or(0.0) * 2.0,
            None,
        )
    } else {
        (style.color(), style.width_px, style.dash.clone())
    }
}

pub fn point_batch_from_style(style: &CircleStyle, stroke: bool) -> (Color, f32) {
    if stroke {
        (
            style.stroke_color().unwrap_or_else(|| style.color()),
            style.radius_px + style.stroke_width_px.unwrap_or(0.0),
        )
    } else {
        (style.color(), style.radius_px)
    }
}

pub fn fill_style_metadata(style: &FillStyle) -> (Option<Color>, Option<FillPatternUniforms>) {
    let color = style.color();
    let pattern = style
        .dots
        .as_ref()
        .map(|dots| FillPatternUniforms::from_style(dots, color));
    (color, pattern)
}

pub fn label_style_metadata(
    style: &TextStyle,
) -> (Color, Option<Color>, Option<f32>, f32, [f32; 2]) {
    let offset = style
        .offset_px
        .unwrap_or(crate::map_rendering_spec::PixelOffset { x: 0.0, y: 0.0 });
    (
        style.color(),
        style.halo_color(),
        style.halo_width_px,
        style.size_px,
        [offset.x, offset.y],
    )
}
