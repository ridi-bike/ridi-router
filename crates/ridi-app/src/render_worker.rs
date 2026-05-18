use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc::Receiver, mpsc::SyncSender, Arc, RwLock};
use std::thread;

use i_triangle::float::triangulatable::Triangulatable;
use tracing::{error, info, warn};

use crate::decoded_mvt_tile::{DecodedMvtTile, MvtFeature};
use crate::map_rendering_spec::{
    FeatureFilter, MapRenderingSpec, RenderInstruction, TextTransform,
};
use crate::mvt_geometry::MvtGeometryLines;
use crate::pmtiles_source::PmtilesSource;
use crate::retained_renderer::*;
use crate::tile_address::TileAddress;

pub struct WorkerFreshness {
    pub latest_generation: AtomicU64,
    pub style_revision: AtomicU64,
    active_tiles: RwLock<HashSet<TileAddress>>,
}

impl WorkerFreshness {
    pub fn new(style_revision: u64) -> Self {
        Self {
            latest_generation: AtomicU64::new(0),
            style_revision: AtomicU64::new(style_revision),
            active_tiles: RwLock::new(HashSet::new()),
        }
    }

    pub fn update_request(&self, generation: WorkGeneration, active_tiles: HashSet<TileAddress>) {
        self.latest_generation
            .store(generation.0, Ordering::Relaxed);
        if let Ok(mut current_active_tiles) = self.active_tiles.write() {
            *current_active_tiles = active_tiles;
        }
    }

    pub fn request_in_active_set(&self, tile: TileAddress) -> bool {
        self.active_tiles
            .read()
            .map(|active_tiles| active_tiles.contains(&tile))
            .unwrap_or(false)
    }
}

pub fn spawn_render_worker(
    spec: MapRenderingSpec,
    work_rx: Receiver<WorkRequest>,
    completed_tx: SyncSender<WorkerMessage>,
    freshness: Arc<WorkerFreshness>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("ridi-render-worker".to_owned())
        .spawn(move || run_worker(spec, work_rx, completed_tx, freshness))
        .expect("render worker thread should spawn")
}

fn run_worker(
    spec: MapRenderingSpec,
    work_rx: Receiver<WorkRequest>,
    completed_tx: SyncSender<WorkerMessage>,
    freshness: Arc<WorkerFreshness>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            error!(%error, "failed to start render worker runtime");
            return;
        }
    };
    let source = match runtime.block_on(PmtilesSource::open_ridi_map()) {
        Ok(source) => source,
        Err(error) => {
            error!(%error, "failed to open worker PMTiles source");
            return;
        }
    };
    info!("render worker ready");

    while let Ok(request) = work_rx.recv() {
        let mut requests = vec![request];
        requests.extend(work_rx.try_iter());

        for request in requests {
            match request {
                WorkRequest::BuildTile {
                    tile,
                    style_revision,
                    generation,
                } => {
                    if !freshness.request_in_active_set(tile) {
                        continue;
                    }
                    build_tile(
                        &spec,
                        &runtime,
                        &source,
                        tile,
                        style_revision,
                        generation,
                        &completed_tx,
                        &freshness,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_tile(
    spec: &MapRenderingSpec,
    runtime: &tokio::runtime::Runtime,
    source: &PmtilesSource,
    tile: TileAddress,
    style_revision: u64,
    generation: WorkGeneration,
    completed_tx: &SyncSender<WorkerMessage>,
    freshness: &WorkerFreshness,
) {
    let tile_key = TileKey {
        address: tile,
        style_revision,
    };
    if is_stale(tile, style_revision, generation, freshness) {
        return;
    }

    let bytes = match runtime.block_on(source.fetch_tile(tile)) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            let _ = completed_tx.send(WorkerMessage::TileMissing {
                tile_key,
                generation,
            });
            return;
        }
        Err(error) => {
            let _ = completed_tx.send(WorkerMessage::TileFailed {
                tile_key,
                generation,
                error: error.to_string(),
            });
            return;
        }
    };
    if is_stale(tile, style_revision, generation, freshness) {
        return;
    }

    let decoded = match DecodedMvtTile::decode(tile, &bytes) {
        Ok(decoded) => decoded,
        Err(error) => {
            let _ = completed_tx.send(WorkerMessage::TileFailed {
                tile_key,
                generation,
                error: error.to_string(),
            });
            return;
        }
    };

    for (zoom_range_index, range) in spec.zoom_ranges.iter().enumerate() {
        if tile.z < range.min_zoom || tile.z > range.max_zoom {
            continue;
        }
        for (rule_index, rule) in range.render_order.iter().enumerate() {
            if is_stale(tile, style_revision, generation, freshness) {
                return;
            }
            let rule_key = RuleKey {
                zoom_range_index,
                rule_index,
                rule_id: rule.id.clone(),
            };
            for part in
                build_rule_parts(&decoded, &rule.layer, &rule.filter, &rule.render, rule_key)
            {
                if is_stale(tile, style_revision, generation, freshness) {
                    return;
                }
                let _ = completed_tx.send(WorkerMessage::CompletedPart {
                    tile_key,
                    rule_key: part.rule_key.clone(),
                    generation,
                    part: Box::new(part),
                });
            }
        }
    }

    if !is_stale(tile, style_revision, generation, freshness) {
        let _ = completed_tx.send(WorkerMessage::TileComplete {
            tile_key,
            generation,
        });
    }
}

fn is_stale(
    tile: TileAddress,
    style_revision: u64,
    generation: WorkGeneration,
    freshness: &WorkerFreshness,
) -> bool {
    style_revision != freshness.style_revision.load(Ordering::Relaxed)
        || generation.0 + 2 < freshness.latest_generation.load(Ordering::Relaxed)
        || !freshness.request_in_active_set(tile)
}

fn build_rule_parts(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    render: &RenderInstruction,
    rule_key: RuleKey,
) -> Vec<RenderedTilePart> {
    match render {
        RenderInstruction::Fill(style) => build_fill_parts(tile, layer, filter, style, rule_key),
        RenderInstruction::Line(style) => build_line_parts(tile, layer, filter, style, rule_key),
        RenderInstruction::Circle(style) => build_point_parts(tile, layer, filter, style, rule_key),
        RenderInstruction::Text(style) => build_label_parts(tile, layer, filter, style, rule_key),
    }
}

fn matching_features<'a>(
    tile: &'a DecodedMvtTile,
    layer: &'a str,
    filter: &'a FeatureFilter,
) -> impl Iterator<Item = MvtFeature<'a>> + 'a {
    tile.features_in_layer(layer)
        .filter(move |feature| matches_filter(*feature, filter))
}

fn build_fill_parts(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::FillStyle,
    rule_key: RuleKey,
) -> Vec<RenderedTilePart> {
    let (color, fill_pattern) = fill_style_metadata(style);
    if color.is_none() && fill_pattern.is_none() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut chunk_index = 0;

    for feature in matching_features(tile, layer, filter) {
        append_feature_fill(
            feature,
            layer,
            &rule_key,
            color,
            fill_pattern.clone(),
            &mut vertices,
            &mut indices,
            &mut chunks,
            &mut chunk_index,
        );
    }

    if !indices.is_empty() {
        chunks.push(make_fill_part(
            rule_key,
            chunk_index,
            layer,
            vertices,
            indices,
            color,
            fill_pattern,
        ));
    }
    chunks
}

#[allow(clippy::too_many_arguments)]
fn append_feature_fill(
    feature: MvtFeature<'_>,
    layer: &str,
    rule_key: &RuleKey,
    color: Option<macroquad::prelude::Color>,
    fill_pattern: Option<FillPatternUniforms>,
    vertices: &mut Vec<TileVertex>,
    indices: &mut Vec<u16>,
    chunks: &mut Vec<RenderedTilePart>,
    chunk_index: &mut usize,
) {
    let mut lines = MvtGeometryLines::default();
    if feature.process_geometry(&mut lines).is_err() {
        return;
    }
    let extent = feature.extent() as f32;

    for line in lines.lines() {
        if line.len() < 3 {
            continue;
        }
        let contour: Vec<[f64; 2]> = line.into_iter().map(|point| [point.0, point.1]).collect();
        let triangulation = [contour].triangulate().to_triangulation::<u32>();
        let needed_vertices = triangulation.points.len();
        if needed_vertices > u16::MAX as usize {
            warn!(needed_vertices, "dropping oversized fill polygon");
            continue;
        }
        if vertices.len() + needed_vertices > u16::MAX as usize && !indices.is_empty() {
            let emitted_vertices = std::mem::take(vertices);
            let emitted_indices = std::mem::take(indices);
            chunks.push(make_fill_part(
                rule_key.clone(),
                *chunk_index,
                layer,
                emitted_vertices,
                emitted_indices,
                color,
                fill_pattern.clone(),
            ));
            *chunk_index += 1;
        }
        let base = vertices.len() as u16;
        vertices.extend(triangulation.points.iter().map(|point| TileVertex {
            local: [point[0] as f32 / extent, point[1] as f32 / extent],
        }));
        for triangle in triangulation.indices.chunks_exact(3) {
            indices.push(base + triangle[0] as u16);
            indices.push(base + triangle[1] as u16);
            indices.push(base + triangle[2] as u16);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn make_fill_part(
    rule_key: RuleKey,
    chunk_index: usize,
    layer: &str,
    vertices: Vec<TileVertex>,
    indices: Vec<u16>,
    color: Option<macroquad::prelude::Color>,
    fill_pattern: Option<FillPatternUniforms>,
) -> RenderedTilePart {
    let cpu_bytes = vertices.len() * std::mem::size_of::<TileVertex>()
        + indices.len() * std::mem::size_of::<u16>();
    RenderedTilePart {
        rule_key: rule_key.clone(),
        chunk_index,
        layer_name: layer.to_owned(),
        kind: RenderedPartKind::FillMesh(FillMesh {
            rule_key,
            chunk_index,
            vertices,
            indices,
            color,
            fill_pattern,
        }),
        cpu_bytes,
    }
}

fn build_line_parts(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::LineStyle,
    rule_key: RuleKey,
) -> Vec<RenderedTilePart> {
    let mut parts = Vec::new();
    if style.casing_color().is_some() || style.casing_width_px.unwrap_or(0.0) > 0.0 {
        parts.push(make_line_part(
            tile,
            layer,
            filter,
            style,
            rule_key.clone(),
            true,
        ));
    }
    parts.push(make_line_part(tile, layer, filter, style, rule_key, false));
    parts
        .into_iter()
        .filter(|part| part.cpu_bytes > 0)
        .collect()
}

fn make_line_part(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::LineStyle,
    rule_key: RuleKey,
    casing: bool,
) -> RenderedTilePart {
    let (color, width_px, dash) = line_batch_from_style(style, casing);
    let mut segments = Vec::new();
    for feature in matching_features(tile, layer, filter) {
        let mut lines = MvtGeometryLines::default();
        if feature.process_geometry(&mut lines).is_err() {
            continue;
        }
        let extent = feature.extent() as f32;
        for line in lines.lines() {
            let points: Vec<_> = line
                .into_iter()
                .map(|point| [point.0 as f32 / extent, point.1 as f32 / extent])
                .collect();
            for pair in points.windows(2) {
                segments.push(LineSegment {
                    from: pair[0],
                    to: pair[1],
                });
            }
        }
    }
    let cpu_bytes = segments.len() * std::mem::size_of::<LineSegment>();
    RenderedTilePart {
        rule_key: rule_key.clone(),
        chunk_index: usize::from(casing),
        layer_name: layer.to_owned(),
        kind: RenderedPartKind::LineBatch(LineBatch {
            color,
            width_px,
            dash,
            segments,
        }),
        cpu_bytes,
    }
}

fn build_point_parts(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::CircleStyle,
    rule_key: RuleKey,
) -> Vec<RenderedTilePart> {
    let mut parts = Vec::new();
    if style.stroke_color().is_some() || style.stroke_width_px.unwrap_or(0.0) > 0.0 {
        parts.push(make_point_part(
            tile,
            layer,
            filter,
            style,
            rule_key.clone(),
            true,
        ));
    }
    parts.push(make_point_part(tile, layer, filter, style, rule_key, false));
    parts
        .into_iter()
        .filter(|part| part.cpu_bytes > 0)
        .collect()
}

fn make_point_part(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::CircleStyle,
    rule_key: RuleKey,
    stroke: bool,
) -> RenderedTilePart {
    let (color, radius_px) = point_batch_from_style(style, stroke);
    let mut points = Vec::new();
    for feature in matching_features(tile, layer, filter) {
        let mut lines = MvtGeometryLines::default();
        if feature.process_geometry(&mut lines).is_err() {
            continue;
        }
        let extent = feature.extent() as f32;
        for line in lines.lines() {
            points.extend(
                line.into_iter()
                    .map(|point| [point.0 as f32 / extent, point.1 as f32 / extent]),
            );
        }
    }
    let cpu_bytes = points.len() * std::mem::size_of::<[f32; 2]>();
    RenderedTilePart {
        rule_key: rule_key.clone(),
        chunk_index: usize::from(stroke),
        layer_name: layer.to_owned(),
        kind: RenderedPartKind::PointBatch(PointBatch {
            color,
            radius_px,
            points,
        }),
        cpu_bytes,
    }
}

fn build_label_parts(
    tile: &DecodedMvtTile,
    layer: &str,
    filter: &FeatureFilter,
    style: &crate::map_rendering_spec::TextStyle,
    rule_key: RuleKey,
) -> Vec<RenderedTilePart> {
    let (color, halo_color, halo_width_px, size_px, offset_px) = label_style_metadata(style);
    let mut labels = Vec::new();
    for feature in matching_features(tile, layer, filter) {
        let Some(text) = label_for_feature(feature, style) else {
            continue;
        };
        let mut lines = MvtGeometryLines::default();
        if feature.process_geometry(&mut lines).is_err() {
            continue;
        }
        let extent = feature.extent() as f32;
        for line in lines.lines() {
            labels.extend(line.into_iter().map(|point| LabelCandidate {
                local: [point.0 as f32 / extent, point.1 as f32 / extent],
                text: text.clone(),
            }));
        }
    }
    let cpu_bytes = labels
        .iter()
        .map(|label| std::mem::size_of::<LabelCandidate>() + label.text.len())
        .sum();
    vec![RenderedTilePart {
        rule_key: rule_key.clone(),
        chunk_index: 0,
        layer_name: layer.to_owned(),
        kind: RenderedPartKind::LabelBatch(LabelBatch {
            color,
            halo_color,
            halo_width_px,
            size_px,
            offset_px,
            labels,
        }),
        cpu_bytes,
    }]
    .into_iter()
    .filter(|part| part.cpu_bytes > 0)
    .collect()
}

fn matches_filter(feature: MvtFeature<'_>, filter: &FeatureFilter) -> bool {
    if let Some(expected) = &filter.kind {
        if string_property(feature, "kind") != Some(expected.as_str()) {
            return false;
        }
    }
    if let Some(expected_values) = &filter.kind_any {
        let Some(actual) = string_property(feature, "kind") else {
            return false;
        };
        if !expected_values.iter().any(|expected| expected == actual) {
            return false;
        }
    }
    if let Some(excluded) = &filter.kind_not {
        if string_property(feature, "kind") == Some(excluded.as_str()) {
            return false;
        }
    }
    if let Some(expected) = &filter.kind_detail {
        if string_property(feature, "kind_detail") != Some(expected.as_str()) {
            return false;
        }
    }
    if let Some(expected_values) = &filter.kind_detail_any {
        let Some(actual) = string_property(feature, "kind_detail") else {
            return false;
        };
        if !expected_values.iter().any(|expected| expected == actual) {
            return false;
        }
    }
    true
}

fn string_property<'a>(feature: MvtFeature<'a>, key: &str) -> Option<&'a str> {
    feature.properties().find_map(|property| {
        if property.key() == key {
            property.value().as_str()
        } else {
            None
        }
    })
}

fn label_for_feature(
    feature: MvtFeature<'_>,
    style: &crate::map_rendering_spec::TextStyle,
) -> Option<String> {
    let label = style
        .fields
        .as_deref()
        .unwrap_or_else(|| {
            std::slice::from_ref(
                style
                    .field
                    .as_ref()
                    .expect("TextStyle requires field or fields"),
            )
        })
        .iter()
        .find_map(|field| string_property(feature, field))?;
    Some(match style.transform {
        Some(TextTransform::Uppercase) => label.to_uppercase(),
        None => label.to_owned(),
    })
}
