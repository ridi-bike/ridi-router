#![allow(dead_code)]
pub mod decoded_mvt_tile;
mod drag;
mod gesture;
mod keyboard_pan;
mod map_rendering_spec;
mod mvt_geometry;
pub mod pmtiles_source;
mod render_cache;
mod render_scheduler;
mod render_worker;
mod retained_renderer;
mod screen_layout;
mod space;
pub mod tile_address;
mod viewport;
pub mod web_mercator_tiles;
mod zoom;

use drag::DragState;
use gesture::{TouchGesture, TouchGestureState};
use keyboard_pan::KeyboardPan;
use macroquad::prelude::*;
use map_rendering_spec::MapRenderingSpec;
use render_cache::{RenderCacheStats, RenderedTileCache};
use render_scheduler::RenderScheduler;
use render_worker::{spawn_render_worker, WorkerFreshness};
use screen_layout::padded_screen_rect;
use space::{GpsCoord, ScreenCoord};
use std::sync::{mpsc::sync_channel, Arc};
use tracing::info;
use viewport::Viewport;
use zoom::zoom_factor;

const MAX_TILE_ZOOM: u8 = 15;

#[macroquad::main("Ridi App")]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
    info!("starting ridi app");

    let rendering_spec = MapRenderingSpec::load();
    info!(
        version = rendering_spec.version,
        source = %rendering_spec.source,
        zoom_range_count = rendering_spec.zoom_ranges.len(),
        "loaded map rendering spec"
    );
    let label_font = rendering_spec.load_font().await;
    let mut viewport = Viewport::centered_at_zoom(
        GpsCoord {
            lat: 57.05270347136819,
            lon: 24.42577097511743,
        },
        8,
    );
    simulate_mouse_with_touch(false);
    let mut touch_gestures = TouchGestureState::default();
    let mut drag = DragState::default();
    let style_revision = rendering_spec.version as u64;
    let freshness = Arc::new(WorkerFreshness::new(style_revision));
    let (work_tx, work_rx) = sync_channel(128);
    let (completed_tx, completed_rx) = sync_channel(512);
    let _render_worker = spawn_render_worker(
        rendering_spec.clone(),
        work_rx,
        completed_tx,
        freshness.clone(),
    );
    let mut render_scheduler = RenderScheduler::new(work_tx, freshness, style_revision);
    let mut render_cache = RenderedTileCache::new();
    let mut frame_time_window_elapsed = 0.0_f32;
    let mut frame_time_window_max_ms = 0.0_f32;
    let mut displayed_max_frame_time_ms = 0.0_f32;

    loop {
        clear_background(rendering_spec.background_color());

        let screen = padded_screen_rect(screen_width(), screen_height(), 0.1);
        let mouse_position = mouse_position();
        let mouse_screen = ScreenCoord {
            x: mouse_position.0 as f64,
            y: mouse_position.1 as f64,
        };
        let previous_viewport = viewport;

        let keyboard_pan = KeyboardPan {
            left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
            right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
            up: is_key_down(KeyCode::Up) || is_key_down(KeyCode::W),
            down: is_key_down(KeyCode::Down) || is_key_down(KeyCode::S),
        };
        viewport.pan_by_fraction(
            screen,
            keyboard_pan.lat_fraction(),
            keyboard_pan.lon_fraction(),
        );

        if let Some(previous_mouse_position) =
            drag.update(is_mouse_button_down(MouseButton::Left), mouse_screen)
        {
            viewport.pan_between_screen_points(screen, previous_mouse_position, mouse_screen);
        }

        let touch_snapshot = touches();
        let touch_gesture = touch_gestures.update(&touch_snapshot);
        match touch_gesture {
            Some(TouchGesture::Drag { previous, current }) => {
                viewport.pan_between_screen_points(screen, previous, current);
            }
            Some(TouchGesture::Pinch {
                previous_center,
                current_center,
                zoom,
            }) => {
                viewport.pan_between_screen_points(screen, previous_center, current_center);
                viewport.zoom_around_screen_point(screen, current_center, zoom);
            }
            None => {}
        }
        viewport.zoom_around_screen_point(
            screen,
            mouse_screen,
            zoom_factor(
                mouse_wheel().1,
                is_key_down(KeyCode::Q),
                is_key_down(KeyCode::E),
            ),
        );

        if viewport != previous_viewport {
            let center = viewport.center();
            info!(
                center_lat = center.lat,
                center_lon = center.lon,
                zoom = viewport.zoom_level(MAX_TILE_ZOOM),
                "viewport changed"
            );
        }

        let space = viewport.space(screen);
        let bounds = space.world;
        let tile_zoom = viewport.zoom_level(MAX_TILE_ZOOM);

        render_scheduler.request_visible_tiles(bounds, tile_zoom);
        render_cache.drain_worker_messages(&completed_rx, &mut render_scheduler);
        render_cache.upload_ready_parts_with_budget();
        render_cache.draw_retained_tiles(
            render_scheduler.visible_tiles(),
            render_scheduler.fallback_tiles(),
            render_scheduler.style_revision(),
            &space,
            label_font.as_ref(),
        );

        let frame_time = get_frame_time();
        let frame_time_ms = frame_time * 1000.0;
        frame_time_window_elapsed += frame_time;
        frame_time_window_max_ms = frame_time_window_max_ms.max(frame_time_ms);
        if frame_time_window_elapsed >= 1.0 {
            displayed_max_frame_time_ms = frame_time_window_max_ms;
            frame_time_window_elapsed = 0.0;
            frame_time_window_max_ms = 0.0;
        }

        draw_debug_overlays(
            &touch_snapshot,
            touch_gesture.as_ref(),
            frame_time_ms,
            displayed_max_frame_time_ms,
            render_cache.stats(),
        );

        next_frame().await;
    }
}

fn draw_debug_overlays(
    touches: &[Touch],
    gesture: Option<&TouchGesture>,
    frame_time_ms: f32,
    max_frame_time_ms: f32,
    render_cache_stats: RenderCacheStats,
) {
    let line_height = 18.0;
    let padding = 8.0;
    let touch_lines = touches.len().min(4);
    let line_count = 5 + touch_lines;
    let width = 520.0;
    let height = padding * 2.0 + line_height * line_count as f32;

    draw_rectangle(8.0, 8.0, width, height, Color::new(0.0, 0.0, 0.0, 0.65));

    let mut y = 8.0 + padding + 14.0;
    draw_text(
        &format!(
            "frame: {:.1} ms ({:.2} fps), max 1s: {:.1} ms",
            frame_time_ms,
            1000.0 / frame_time_ms.max(0.001),
            max_frame_time_ms,
        ),
        16.0,
        y,
        18.0,
        WHITE,
    );

    y += line_height;
    draw_text(&format!("touches: {}", touches.len()), 16.0, y, 18.0, WHITE);

    y += line_height;
    draw_text(
        &format!(
            "gesture: {}",
            gesture
                .map(|gesture| format!("{gesture:?}"))
                .unwrap_or_else(|| "none".to_owned()),
        ),
        16.0,
        y,
        18.0,
        WHITE,
    );

    y += line_height;
    draw_text(
        &format!(
            "retained cache: tiles={} cpu={:.1}MB gpu={:.1}MB pending_uploads={}",
            render_cache_stats.tiles,
            render_cache_stats.cpu_bytes as f32 / (1024.0 * 1024.0),
            render_cache_stats.gpu_bytes as f32 / (1024.0 * 1024.0),
            render_cache_stats.pending_uploads,
        ),
        16.0,
        y,
        18.0,
        WHITE,
    );

    for touch in touches.iter().take(4) {
        y += line_height;
        draw_text(
            &format!(
                "touch id={} phase={} x={:.0} y={:.0}",
                touch.id,
                touch_phase_name(touch.phase),
                touch.position.x,
                touch.position.y,
            ),
            16.0,
            y,
            18.0,
            WHITE,
        );
    }
}

fn touch_phase_name(phase: TouchPhase) -> &'static str {
    match phase {
        TouchPhase::Started => "started",
        TouchPhase::Stationary => "stationary",
        TouchPhase::Moved => "moved",
        TouchPhase::Ended => "ended",
        TouchPhase::Cancelled => "cancelled",
    }
}
