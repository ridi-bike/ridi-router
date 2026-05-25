use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use macroquad::miniquad::*;
use macroquad::prelude::*;
use tracing::{debug, warn};

use crate::retained_renderer::*;
use crate::space::{lat_to_mercator_y, lon_to_mercator_x, Space};
use crate::tile_address::TileAddress;

const TARGET_CACHE_BYTES: usize = 100 * 1024 * 1024;
const HARD_CACHE_BYTES: usize = 200 * 1024 * 1024;
const UPLOAD_BUDGET: Duration = Duration::from_millis(3);
const MAX_U16_VERTICES: usize = u16::MAX as usize;
const MAP_LABEL_TEXT_SCALE: f32 = 1.5;

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderCacheStats {
    pub tiles: usize,
    pub cpu_bytes: usize,
    pub gpu_bytes: usize,
    pub pending_uploads: usize,
}

pub struct RenderedTileCache {
    tiles: HashMap<TileKey, RenderedTile>,
    pending_uploads: Vec<(TileKey, RenderedTilePart)>,
    pending_deletes: Vec<(BufferId, BufferId)>,
    use_counter: u64,
    cpu_bytes: usize,
    gpu_bytes: usize,
    fill_pipeline: Option<Pipeline>,
    line_pipeline: Option<Pipeline>,
    point_pipeline: Option<Pipeline>,
    replacement_policy: ReplacementPolicy,
}

struct RenderedTile {
    parts: Vec<CachedPart>,
    complete: bool,
    last_used: u64,
    cpu_bytes: usize,
    gpu_bytes: usize,
}

struct CachedPart {
    rule_key: RuleKey,
    chunk_index: usize,
    kind: CachedPartKind,
    cpu_bytes: usize,
    gpu_bytes: usize,
}

enum CachedPartKind {
    GpuFill(GpuFillPart),
    GpuLine(GpuLinePart),
    GpuPoint(GpuPointPart),
    LabelBatch(LabelBatch),
}

struct GpuFillPart {
    color: Option<Color>,
    fill_pattern: Option<FillPatternUniforms>,
    bindings: Bindings,
    index_count: i32,
}

struct GpuLinePart {
    color: Color,
    bindings: Bindings,
    index_count: i32,
}

struct GpuPointPart {
    color: Color,
    point_size_px: f32,
    bindings: Bindings,
    index_count: i32,
}

#[repr(C)]
struct TileUniforms {
    viewport: (f32, f32, f32, f32),
    tile: (f32, f32, f32, f32),
    color: (f32, f32, f32, f32),
    aux: (f32, f32, f32, f32),
}

impl RenderedTileCache {
    pub fn new() -> Self {
        Self {
            tiles: HashMap::new(),
            pending_uploads: Vec::new(),
            pending_deletes: Vec::new(),
            use_counter: 0,
            cpu_bytes: 0,
            gpu_bytes: 0,
            fill_pipeline: None,
            line_pipeline: None,
            point_pipeline: None,
            replacement_policy: ReplacementPolicy::TileByTile,
        }
    }

    pub fn drain_worker_messages(
        &mut self,
        rx: &Receiver<WorkerMessage>,
        scheduler: &mut crate::render_scheduler::RenderScheduler,
    ) {
        while let Ok(message) = rx.try_recv() {
            match message {
                WorkerMessage::CompletedPart {
                    tile_key,
                    generation,
                    part,
                    ..
                } => {
                    if tile_key.style_revision != scheduler.style_revision()
                        || generation.0 + 4 < scheduler.generation().0
                    {
                        continue;
                    }
                    scheduler.mark_partial(tile_key);
                    self.pending_uploads.push((tile_key, *part));
                }
                WorkerMessage::TileComplete {
                    tile_key,
                    generation,
                } => {
                    if generation.0 + 4 >= scheduler.generation().0 {
                        scheduler.mark_complete(tile_key);
                        self.tile_mut(tile_key).complete = true;
                    }
                }
                WorkerMessage::TileMissing {
                    tile_key,
                    generation,
                } => {
                    if generation.0 + 4 >= scheduler.generation().0 {
                        scheduler.mark_missing(tile_key);
                    }
                }
                WorkerMessage::TileFailed {
                    tile_key,
                    generation,
                    error,
                } => {
                    if generation.0 + 4 >= scheduler.generation().0 {
                        warn!(?tile_key, %error, "render worker tile failed");
                        scheduler.mark_failed(tile_key);
                    }
                }
            }
        }
    }

    pub fn upload_ready_parts_with_budget(&mut self) {
        let start = Instant::now();
        let gl = unsafe { get_internal_gl() };
        while let Some((vertex_buffer, index_buffer)) = self.pending_deletes.pop() {
            gl.quad_context.delete_buffer(vertex_buffer);
            gl.quad_context.delete_buffer(index_buffer);
        }
        self.ensure_pipelines(&mut *gl.quad_context);
        while start.elapsed() < UPLOAD_BUDGET {
            let Some((tile_key, part)) = self.pending_uploads.pop() else {
                break;
            };
            self.insert_part(&mut *gl.quad_context, tile_key, part);
        }
    }

    pub fn draw_retained_tiles(
        &mut self,
        visible: &[TileAddress],
        fallback: &[TileAddress],
        style_revision: u64,
        space: &Space,
        label_font: Option<&Font>,
    ) {
        self.use_counter = self.use_counter.wrapping_add(1);
        let draw_tiles = self.best_available_tiles(visible, fallback, style_revision);

        {
            let mut gl = unsafe { get_internal_gl() };
            gl.flush();
            self.ensure_pipelines(&mut *gl.quad_context);
            gl.quad_context.begin_default_pass(PassAction::Nothing);
            for rule_key in self.sorted_rule_keys(&draw_tiles) {
                for tile_key in &draw_tiles {
                    if let Some(tile) = self.tiles.get_mut(tile_key) {
                        tile.last_used = self.use_counter;
                        for part in tile
                            .parts
                            .iter()
                            .filter(|part| part.rule_key == rule_key)
                            .filter(|part| !matches!(part.kind, CachedPartKind::LabelBatch(_)))
                        {
                            match &part.kind {
                                CachedPartKind::GpuFill(fill) => {
                                    if let Some(pipeline) = self.fill_pipeline {
                                        gl.quad_context.apply_pipeline(&pipeline);
                                        draw_gpu_fill(
                                            &mut *gl.quad_context,
                                            fill,
                                            tile_key.address,
                                            space,
                                        );
                                    }
                                }
                                CachedPartKind::GpuLine(line) => {
                                    if let Some(pipeline) = self.line_pipeline {
                                        gl.quad_context.apply_pipeline(&pipeline);
                                        draw_gpu_line(
                                            &mut *gl.quad_context,
                                            line,
                                            tile_key.address,
                                            space,
                                        );
                                    }
                                }
                                CachedPartKind::GpuPoint(point) => {
                                    if let Some(pipeline) = self.point_pipeline {
                                        gl.quad_context.apply_pipeline(&pipeline);
                                        draw_gpu_point(
                                            &mut *gl.quad_context,
                                            point,
                                            tile_key.address,
                                            space,
                                        );
                                    }
                                }
                                CachedPartKind::LabelBatch(_) => {}
                            }
                        }
                    }
                }
            }
            gl.quad_context.end_render_pass();
        }

        set_default_camera();
        for rule_key in self.sorted_rule_keys(&draw_tiles) {
            for tile_key in &draw_tiles {
                if let Some(tile) = self.tiles.get(tile_key) {
                    for part in tile
                        .parts
                        .iter()
                        .filter(|part| part.rule_key == rule_key)
                        .filter_map(|part| match &part.kind {
                            CachedPartKind::LabelBatch(batch) => Some(batch),
                            _ => None,
                        })
                    {
                        draw_labels(part, tile_key.address, space, label_font);
                    }
                }
            }
        }
        self.evict(visible, fallback, style_revision);
    }

    pub fn best_available_tiles(
        &self,
        visible: &[TileAddress],
        fallback: &[TileAddress],
        style_revision: u64,
    ) -> Vec<TileKey> {
        let mut keys = Vec::new();
        match self.replacement_policy {
            ReplacementPolicy::TileByTile | ReplacementPolicy::LayerByLayer => {
                for &address in visible {
                    let key = TileKey {
                        address,
                        style_revision,
                    };
                    if self.tile_drawable(key) {
                        keys.push(key);
                    } else if let Some(parent) = parent_tile(address) {
                        let parent_key = TileKey {
                            address: parent,
                            style_revision,
                        };
                        if self.tile_drawable(parent_key) {
                            keys.push(parent_key);
                            continue;
                        }
                        keys.extend(self.fallback_keys_for_missing_tile(address, fallback, style_revision));
                    } else {
                        keys.extend(self.fallback_keys_for_missing_tile(address, fallback, style_revision));
                    }
                }
            }
            ReplacementPolicy::WholeViewportWhenReady => {
                let all_ready = visible.iter().all(|&address| {
                    self.tile_drawable(TileKey {
                        address,
                        style_revision,
                    })
                });
                if all_ready {
                    keys.extend(visible.iter().map(|&address| TileKey {
                        address,
                        style_revision,
                    }));
                } else {
                    keys.extend(fallback.iter().filter_map(|&address| {
                        let key = TileKey {
                            address,
                            style_revision,
                        };
                        self.tile_drawable(key).then_some(key)
                    }));
                }
            }
        }
        if keys.is_empty() {
            keys.extend(fallback.iter().filter_map(|&address| {
                let key = TileKey {
                    address,
                    style_revision,
                };
                self.tile_drawable(key).then_some(key)
            }));
        }
        keys.sort_by_key(|key| (key.address.z, key.address.y, key.address.x));
        keys.dedup();
        keys
    }

    fn tile_drawable(&self, key: TileKey) -> bool {
        self.tiles
            .get(&key)
            .is_some_and(|tile| tile.complete && !tile.parts.is_empty())
            && !self
                .pending_uploads
                .iter()
                .any(|(pending_key, _)| *pending_key == key)
    }

    fn fallback_keys_for_missing_tile(
        &self,
        address: TileAddress,
        fallback: &[TileAddress],
        style_revision: u64,
    ) -> Vec<TileKey> {
        fallback
            .iter()
            .copied()
            .filter(|fallback_address| tiles_overlap(address, *fallback_address))
            .filter_map(|fallback_address| {
                let key = TileKey {
                    address: fallback_address,
                    style_revision,
                };
                self.tile_drawable(key).then_some(key)
            })
            .collect()
    }

    pub fn stats(&self) -> RenderCacheStats {
        RenderCacheStats {
            tiles: self.tiles.len(),
            cpu_bytes: self.cpu_bytes,
            gpu_bytes: self.gpu_bytes,
            pending_uploads: self.pending_uploads.len(),
        }
    }

    fn ensure_pipelines(&mut self, ctx: &mut dyn RenderingBackend) {
        if self.fill_pipeline.is_none() {
            self.fill_pipeline = Some(new_pipeline(
                ctx,
                FILL_VERTEX_SHADER,
                FILL_FRAGMENT_SHADER,
                PrimitiveType::Triangles,
            ));
        }
        if self.line_pipeline.is_none() {
            self.line_pipeline = Some(new_pipeline(
                ctx,
                SIMPLE_VERTEX_SHADER,
                SIMPLE_FRAGMENT_SHADER,
                PrimitiveType::Lines,
            ));
        }
        if self.point_pipeline.is_none() {
            self.point_pipeline = Some(new_pipeline(
                ctx,
                POINT_VERTEX_SHADER,
                POINT_FRAGMENT_SHADER,
                PrimitiveType::Points,
            ));
        }
    }

    fn insert_part(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        tile_key: TileKey,
        part: RenderedTilePart,
    ) {
        let cached_parts = match part.kind {
            RenderedPartKind::FillMesh(mesh) => {
                self.upload_fill(ctx, part.rule_key, part.chunk_index, mesh, part.cpu_bytes)
            }
            RenderedPartKind::LineBatch(batch) => {
                self.upload_line(ctx, part.rule_key, part.chunk_index, batch, part.cpu_bytes)
            }
            RenderedPartKind::PointBatch(batch) => {
                self.upload_point(ctx, part.rule_key, part.chunk_index, batch, part.cpu_bytes)
            }
            RenderedPartKind::LabelBatch(batch) => vec![CachedPart {
                rule_key: part.rule_key,
                chunk_index: part.chunk_index,
                kind: CachedPartKind::LabelBatch(batch),
                cpu_bytes: part.cpu_bytes,
                gpu_bytes: 0,
            }],
        };

        for cached in cached_parts {
            self.cpu_bytes += cached.cpu_bytes;
            self.gpu_bytes += cached.gpu_bytes;
            let tile = self.tile_mut(tile_key);
            tile.cpu_bytes += cached.cpu_bytes;
            tile.gpu_bytes += cached.gpu_bytes;
            tile.parts.push(cached);
        }
    }

    fn upload_fill(
        &self,
        ctx: &mut dyn RenderingBackend,
        rule_key: RuleKey,
        chunk_index: usize,
        mesh: FillMesh,
        cpu_bytes: usize,
    ) -> Vec<CachedPart> {
        if mesh.indices.is_empty() || mesh.vertices.is_empty() {
            return Vec::new();
        }
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&mesh.vertices),
        );
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&mesh.indices),
        );
        let gpu_bytes = mesh.vertices.len() * std::mem::size_of::<TileVertex>()
            + mesh.indices.len() * std::mem::size_of::<u16>();
        vec![CachedPart {
            rule_key,
            chunk_index,
            kind: CachedPartKind::GpuFill(GpuFillPart {
                color: mesh.color,
                fill_pattern: mesh.fill_pattern,
                bindings: Bindings {
                    vertex_buffers: vec![vertex_buffer],
                    index_buffer,
                    images: vec![],
                },
                index_count: mesh.indices.len() as i32,
            }),
            cpu_bytes,
            gpu_bytes,
        }]
    }

    fn upload_line(
        &self,
        ctx: &mut dyn RenderingBackend,
        rule_key: RuleKey,
        chunk_index: usize,
        batch: LineBatch,
        cpu_bytes: usize,
    ) -> Vec<CachedPart> {
        let mut parts = Vec::new();
        for (chunk_offset, chunk) in batch.segments.chunks(MAX_U16_VERTICES / 2).enumerate() {
            if chunk.is_empty() {
                continue;
            }
            let mut vertices = Vec::with_capacity(chunk.len() * 2);
            let mut indices = Vec::with_capacity(chunk.len() * 2);
            for segment in chunk {
                let base = vertices.len() as u16;
                vertices.push(TileVertex {
                    local: segment.from,
                });
                vertices.push(TileVertex { local: segment.to });
                indices.push(base);
                indices.push(base + 1);
            }
            parts.push(self.upload_simple_gpu_part(
                ctx,
                rule_key.clone(),
                chunk_index + chunk_offset,
                vertices,
                indices,
                CachedSimpleKind::Line { color: batch.color },
                cpu_bytes / batch.segments.len().max(1) * chunk.len(),
            ));
        }
        parts
    }

    fn upload_point(
        &self,
        ctx: &mut dyn RenderingBackend,
        rule_key: RuleKey,
        chunk_index: usize,
        batch: PointBatch,
        cpu_bytes: usize,
    ) -> Vec<CachedPart> {
        let mut parts = Vec::new();
        for (chunk_offset, chunk) in batch.points.chunks(MAX_U16_VERTICES).enumerate() {
            if chunk.is_empty() {
                continue;
            }
            let vertices: Vec<_> = chunk.iter().map(|&local| TileVertex { local }).collect();
            let indices: Vec<_> = (0..vertices.len() as u16).collect();
            parts.push(self.upload_simple_gpu_part(
                ctx,
                rule_key.clone(),
                chunk_index + chunk_offset,
                vertices,
                indices,
                CachedSimpleKind::Point {
                    color: batch.color,
                    point_size_px: batch.radius_px * 2.0,
                },
                cpu_bytes / batch.points.len().max(1) * chunk.len(),
            ));
        }
        parts
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_simple_gpu_part(
        &self,
        ctx: &mut dyn RenderingBackend,
        rule_key: RuleKey,
        chunk_index: usize,
        vertices: Vec<TileVertex>,
        indices: Vec<u16>,
        kind: CachedSimpleKind,
        cpu_bytes: usize,
    ) -> CachedPart {
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        let gpu_bytes = vertices.len() * std::mem::size_of::<TileVertex>()
            + indices.len() * std::mem::size_of::<u16>();
        let bindings = Bindings {
            vertex_buffers: vec![vertex_buffer],
            index_buffer,
            images: vec![],
        };
        let cached_kind = match kind {
            CachedSimpleKind::Line { color } => CachedPartKind::GpuLine(GpuLinePart {
                color,
                bindings,
                index_count: indices.len() as i32,
            }),
            CachedSimpleKind::Point {
                color,
                point_size_px,
            } => CachedPartKind::GpuPoint(GpuPointPart {
                color,
                point_size_px,
                bindings,
                index_count: indices.len() as i32,
            }),
        };
        CachedPart {
            rule_key,
            chunk_index,
            kind: cached_kind,
            cpu_bytes,
            gpu_bytes,
        }
    }

    fn tile_mut(&mut self, key: TileKey) -> &mut RenderedTile {
        self.tiles.entry(key).or_insert_with(|| RenderedTile {
            parts: Vec::new(),
            complete: false,
            last_used: self.use_counter,
            cpu_bytes: 0,
            gpu_bytes: 0,
        })
    }

    fn sorted_rule_keys(&self, draw_tiles: &[TileKey]) -> Vec<RuleKey> {
        let mut rules = Vec::new();
        for key in draw_tiles {
            if let Some(tile) = self.tiles.get(key) {
                rules.extend(tile.parts.iter().map(|part| part.rule_key.clone()));
            }
        }
        rules.sort();
        rules.dedup();
        rules
    }

    fn evict(&mut self, visible: &[TileAddress], fallback: &[TileAddress], style_revision: u64) {
        if self.cpu_bytes + self.gpu_bytes <= TARGET_CACHE_BYTES {
            return;
        }
        let protected: HashSet<_> = visible
            .iter()
            .chain(fallback.iter())
            .map(|&address| TileKey {
                address,
                style_revision,
            })
            .collect();
        let center = visible_center(visible);
        let mut candidates: Vec<_> = self
            .tiles
            .iter()
            .filter(|(key, _)| {
                key.style_revision != style_revision
                    || !protected.contains(key)
                    || self.cpu_bytes + self.gpu_bytes > HARD_CACHE_BYTES
            })
            .map(|(key, tile)| {
                let dx = key.address.x as i64 - center.0 as i64;
                let dy = key.address.y as i64 - center.1 as i64;
                let distance = dx * dx + dy * dy;
                (
                    *key,
                    key.style_revision == style_revision,
                    std::cmp::Reverse(distance),
                    tile.last_used,
                )
            })
            .collect();
        candidates.sort_by_key(|(_, current_style, distance, last_used)| {
            (*current_style, *distance, *last_used)
        });
        for (key, ..) in candidates {
            if self.cpu_bytes + self.gpu_bytes <= TARGET_CACHE_BYTES {
                break;
            }
            self.remove_tile(key);
        }
    }

    fn remove_tile(&mut self, key: TileKey) {
        let Some(tile) = self.tiles.remove(&key) else {
            return;
        };
        self.cpu_bytes = self.cpu_bytes.saturating_sub(tile.cpu_bytes);
        self.gpu_bytes = self.gpu_bytes.saturating_sub(tile.gpu_bytes);
        for part in tile.parts {
            match part.kind {
                CachedPartKind::GpuFill(fill) => self.queue_delete(fill.bindings),
                CachedPartKind::GpuLine(line) => self.queue_delete(line.bindings),
                CachedPartKind::GpuPoint(point) => self.queue_delete(point.bindings),
                CachedPartKind::LabelBatch(_) => {}
            }
        }
        debug!(?key, "evicted retained tile");
    }

    fn queue_delete(&mut self, bindings: Bindings) {
        if let Some(vertex) = bindings.vertex_buffers.first().copied() {
            self.pending_deletes.push((vertex, bindings.index_buffer));
        }
    }
}

enum CachedSimpleKind {
    Line { color: Color },
    Point { color: Color, point_size_px: f32 },
}

fn new_pipeline(
    ctx: &mut dyn RenderingBackend,
    vertex: &str,
    fragment: &str,
    primitive_type: PrimitiveType,
) -> Pipeline {
    let shader = ctx
        .new_shader(
            ShaderSource::Glsl { vertex, fragment },
            ShaderMeta {
                images: vec![],
                uniforms: UniformBlockLayout {
                    uniforms: vec![
                        UniformDesc::new("viewport", UniformType::Float4),
                        UniformDesc::new("tile", UniformType::Float4),
                        UniformDesc::new("color", UniformType::Float4),
                        UniformDesc::new("aux", UniformType::Float4),
                    ],
                },
            },
        )
        .expect("retained renderer shader should compile");
    ctx.new_pipeline(
        &[BufferLayout::default()],
        &[VertexAttribute::new("local", VertexFormat::Float2)],
        shader,
        PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            primitive_type,
            ..Default::default()
        },
    )
}

fn draw_gpu_fill(
    ctx: &mut dyn RenderingBackend,
    fill: &GpuFillPart,
    address: TileAddress,
    space: &Space,
) {
    let base_color = fill
        .fill_pattern
        .as_ref()
        .and_then(|pattern| pattern.base_color)
        .or(fill.color)
        .unwrap_or(Color::new(0.0, 0.0, 0.0, 0.0));
    let aux = fill
        .fill_pattern
        .as_ref()
        .map(|pattern| (1.0_f32, pattern.spacing_px, pattern.radius_px, 0.0_f32))
        .unwrap_or((0.0, 1.0, 0.0, 0.0));
    let color = fill
        .fill_pattern
        .as_ref()
        .map(|pattern| pattern.dot_color)
        .unwrap_or(base_color);
    draw_gpu_indexed(
        ctx,
        &fill.bindings,
        fill.index_count,
        address,
        space,
        base_color,
        color_tuple(color),
        aux,
    );
}

fn draw_gpu_line(
    ctx: &mut dyn RenderingBackend,
    line: &GpuLinePart,
    address: TileAddress,
    space: &Space,
) {
    draw_gpu_indexed(
        ctx,
        &line.bindings,
        line.index_count,
        address,
        space,
        line.color,
        (0.0, 0.0, 0.0, 0.0),
        (0.0, 0.0, 0.0, 0.0),
    );
}

fn draw_gpu_point(
    ctx: &mut dyn RenderingBackend,
    point: &GpuPointPart,
    address: TileAddress,
    space: &Space,
) {
    draw_gpu_indexed(
        ctx,
        &point.bindings,
        point.index_count,
        address,
        space,
        point.color,
        (0.0, 0.0, 0.0, 0.0),
        (point.point_size_px, 0.0, 0.0, 0.0),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_gpu_indexed(
    ctx: &mut dyn RenderingBackend,
    bindings: &Bindings,
    index_count: i32,
    address: TileAddress,
    space: &Space,
    color: Color,
    aux_color: (f32, f32, f32, f32),
    aux: (f32, f32, f32, f32),
) {
    let min_x = lon_to_mercator_x(space.world.min.lon) as f32;
    let max_x = lon_to_mercator_x(space.world.max.lon) as f32;
    let min_y = lat_to_mercator_y(space.world.max.lat) as f32;
    let max_y = lat_to_mercator_y(space.world.min.lat) as f32;
    let n = 2.0_f32.powi(address.z as i32);
    ctx.apply_bindings(bindings);
    ctx.apply_uniforms(UniformsSource::table(&TileUniforms {
        viewport: (min_x, min_y, max_x - min_x, max_y - min_y),
        tile: (address.x as f32, address.y as f32, n, 0.0),
        color: color_tuple(color),
        aux: (aux.0, aux.1, aux.2, aux_color.3),
    }));
    ctx.draw(0, index_count, 1);
}

fn draw_labels(batch: &LabelBatch, address: TileAddress, space: &Space, label_font: Option<&Font>) {
    for label in &batch.labels {
        let point = local_to_screen(space, address, label.local);
        let x = point.x + batch.offset_px[0];
        let y = point.y + batch.offset_px[1];
        if let Some(halo_color) = batch.halo_color {
            let halo = batch.halo_width_px.unwrap_or(1.0);
            for (dx, dy) in [(-halo, 0.0), (halo, 0.0), (0.0, -halo), (0.0, halo)] {
                draw_text_ex(
                    &label.text,
                    x + dx,
                    y + dy,
                    TextParams {
                        font: label_font,
                        font_size: (batch.size_px * MAP_LABEL_TEXT_SCALE)
                            .round()
                            .clamp(1.0, u16::MAX as f32) as u16,
                        color: halo_color,
                        ..Default::default()
                    },
                );
            }
        }
        draw_text_ex(
            &label.text,
            x,
            y,
            TextParams {
                font: label_font,
                font_size: (batch.size_px * MAP_LABEL_TEXT_SCALE)
                    .round()
                    .clamp(1.0, u16::MAX as f32) as u16,
                color: batch.color,
                ..Default::default()
            },
        );
    }
}

fn parent_tile(address: TileAddress) -> Option<TileAddress> {
    (address.z > 0).then(|| TileAddress::new(address.z - 1, address.x / 2, address.y / 2))
}

fn tiles_overlap(a: TileAddress, b: TileAddress) -> bool {
    if a.z == b.z {
        return a.x == b.x && a.y == b.y;
    }
    if a.z > b.z {
        let shift = u32::from(a.z - b.z);
        return (a.x >> shift) == b.x && (a.y >> shift) == b.y;
    }
    let shift = u32::from(b.z - a.z);
    (b.x >> shift) == a.x && (b.y >> shift) == a.y
}

fn visible_center(tiles: &[TileAddress]) -> (u32, u32) {
    if tiles.is_empty() {
        return (0, 0);
    }
    let x = tiles.iter().map(|tile| tile.x as u64).sum::<u64>() / tiles.len() as u64;
    let y = tiles.iter().map(|tile| tile.y as u64).sum::<u64>() / tiles.len() as u64;
    (x as u32, y as u32)
}

fn color_tuple(color: Color) -> (f32, f32, f32, f32) {
    (color.r, color.g, color.b, color.a)
}

const SIMPLE_VERTEX_SHADER: &str = r#"#version 100
attribute vec2 local;
uniform vec4 viewport;
uniform vec4 tile;
uniform vec4 color;
uniform vec4 aux;
varying lowp vec4 v_color;
void main() {
    vec2 world = (tile.xy + local) / tile.z;
    vec2 normalized = (world - viewport.xy) / viewport.zw;
    vec2 clip = vec2(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0);
    gl_Position = vec4(clip, 0.0, 1.0);
    v_color = color;
}
"#;

const SIMPLE_FRAGMENT_SHADER: &str = r#"#version 100
precision mediump float;
varying lowp vec4 v_color;
void main() {
    if (v_color.a <= 0.001) {
        discard;
    }
    gl_FragColor = v_color;
}
"#;

const POINT_VERTEX_SHADER: &str = r#"#version 100
attribute vec2 local;
uniform vec4 viewport;
uniform vec4 tile;
uniform vec4 color;
uniform vec4 aux;
varying lowp vec4 v_color;
void main() {
    vec2 world = (tile.xy + local) / tile.z;
    vec2 normalized = (world - viewport.xy) / viewport.zw;
    vec2 clip = vec2(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0);
    gl_Position = vec4(clip, 0.0, 1.0);
    gl_PointSize = max(aux.x, 1.0);
    v_color = color;
}
"#;

const POINT_FRAGMENT_SHADER: &str = r#"#version 100
precision mediump float;
varying lowp vec4 v_color;
void main() {
    vec2 centered = gl_PointCoord - vec2(0.5);
    if (dot(centered, centered) > 0.25) {
        discard;
    }
    gl_FragColor = v_color;
}
"#;

const FILL_VERTEX_SHADER: &str = r#"#version 100
attribute vec2 local;
uniform vec4 viewport;
uniform vec4 tile;
uniform vec4 color;
uniform vec4 aux;
varying lowp vec4 v_color;
void main() {
    vec2 world = (tile.xy + local) / tile.z;
    vec2 normalized = (world - viewport.xy) / viewport.zw;
    vec2 clip = vec2(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0);
    gl_Position = vec4(clip, 0.0, 1.0);
    v_color = color;
}
"#;

const FILL_FRAGMENT_SHADER: &str = r#"#version 100
precision mediump float;
uniform vec4 aux;
varying lowp vec4 v_color;
void main() {
    vec4 out_color = v_color;
    if (aux.x > 0.5) {
        vec2 grid = fract(gl_FragCoord.xy / max(aux.y, 1.0));
        float dist = distance(grid, vec2(0.5)) * max(aux.y, 1.0);
        if (dist > aux.z) {
            if (out_color.a <= 0.001) {
                discard;
            }
        }
    }
    if (out_color.a <= 0.001) {
        discard;
    }
    gl_FragColor = out_color;
}
"#;
