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
mod ui;
mod user_location;
mod viewport;
pub mod web_mercator_tiles;
mod zoom;

use drag::DragState;
use gesture::{TouchGesture, TouchGestureState};
use keyboard_pan::KeyboardPan;
use macroquad::prelude::*;
use macroquad::ui::root_ui;
use map_rendering_spec::MapRenderingSpec;
use render_cache::{RenderCacheStats, RenderedTileCache};
use render_scheduler::RenderScheduler;
use render_worker::{spawn_render_worker, WorkerFreshness};
use screen_layout::padded_screen_rect;
use space::{GpsCoord, ScreenCoord};
use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc::sync_channel, Arc},
};
use tracing::info;
use user_location::UserLocationState;
use viewport::Viewport;
use zoom::zoom_factor;

const MAX_TILE_ZOOM: u8 = 15;
const DEBUG_TEXT_SIZE_PX: f32 = 36.0;
const DEBUG_LINE_HEIGHT_PX: f32 = 36.0;

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
    let mut user_location = UserLocationState::default();
    let ui_skin = ui::large_text_skin();
    let mut app_ui = ui::UiState::default();
    let mut mouse_press_position: Option<Vec2> = None;
    let mut mouse_started_on_ui = false;
    let mut ui_touch_ids = HashSet::new();
    let mut map_touch_starts = HashMap::new();

    loop {
        clear_background(rendering_spec.background_color());

        let screen = padded_screen_rect(screen_width(), screen_height(), 0.1);
        let mouse_position = mouse_position();
        let mouse_screen = ScreenCoord {
            x: mouse_position.0 as f64,
            y: mouse_position.1 as f64,
        };
        let ui_layout = app_ui.layout(screen_width(), screen_height());
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

        root_ui().push_skin(&ui_skin);
        let ui_actions = app_ui.draw(user_location);
        root_ui().pop_skin();
        if ui_actions.locate_requested {
            user_location.request();
        }

        let pointer_over_ui =
            ui_layout.point_over_ui(vec2(mouse_screen.x as f32, mouse_screen.y as f32), &app_ui);
        if is_mouse_button_pressed(MouseButton::Left) {
            mouse_started_on_ui = pointer_over_ui;
            mouse_press_position = Some(vec2(mouse_screen.x as f32, mouse_screen.y as f32));
        }
        if is_mouse_button_released(MouseButton::Left) {
            if !mouse_started_on_ui && !pointer_over_ui {
                if let Some(press_position) = mouse_press_position {
                    let release_position = vec2(mouse_screen.x as f32, mouse_screen.y as f32);
                    if press_position.distance(release_position) <= ui::TAP_MAX_DISTANCE_PX {
                        let tap_space = viewport.space(screen);
                        app_ui.tap_chooser = Some(ui::TapChooser {
                            coord: tap_space.map_screen_to_world(
                                mouse_screen,
                                screen_width(),
                                screen_height(),
                            ),
                        });
                    }
                }
            }
            mouse_started_on_ui = false;
            mouse_press_position = None;
        }

        if ui_actions.chooser_selection_consumed_input {
            mouse_started_on_ui = true;
            mouse_press_position = None;
            map_touch_starts.clear();
            for touch in touches() {
                ui_touch_ids.insert(touch.id);
            }
        }

        if !pointer_over_ui && !mouse_started_on_ui {
            if let Some(previous_mouse_position) =
                drag.update(is_mouse_button_down(MouseButton::Left), mouse_screen)
            {
                viewport.pan_between_screen_points(screen, previous_mouse_position, mouse_screen);
            }
        } else {
            drag.update(false, mouse_screen);
        }

        let touch_snapshot = touches();
        for touch in &touch_snapshot {
            let touch_over_ui = ui_layout.point_over_ui(touch.position, &app_ui);
            if touch.phase == TouchPhase::Started {
                if touch_over_ui {
                    ui_touch_ids.insert(touch.id);
                } else {
                    map_touch_starts.insert(touch.id, touch.position);
                }
            }
            if touch.phase == TouchPhase::Ended {
                if !ui_actions.chooser_selection_consumed_input && !ui_touch_ids.contains(&touch.id)
                {
                    if let Some(start_position) = map_touch_starts.get(&touch.id) {
                        if start_position.distance(touch.position) <= ui::TAP_MAX_DISTANCE_PX {
                            let tap_space = viewport.space(screen);
                            app_ui.tap_chooser = Some(ui::TapChooser {
                                coord: tap_space.map_screen_to_world(
                                    ScreenCoord {
                                        x: touch.position.x as f64,
                                        y: touch.position.y as f64,
                                    },
                                    screen_width(),
                                    screen_height(),
                                ),
                            });
                        }
                    }
                }
                map_touch_starts.remove(&touch.id);
            }
            if touch.phase == TouchPhase::Cancelled {
                map_touch_starts.remove(&touch.id);
            }
        }
        let map_touches: Vec<Touch> = touch_snapshot
            .iter()
            .filter(|touch| {
                !ui_touch_ids.contains(&touch.id)
                    && !ui_layout.point_over_ui(touch.position, &app_ui)
            })
            .cloned()
            .collect();
        ui_touch_ids.retain(|id| {
            touch_snapshot.iter().any(|touch| {
                touch.id == *id && !matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled)
            })
        });
        let touch_gesture = touch_gestures.update(&map_touches);
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
        if !pointer_over_ui {
            viewport.zoom_around_screen_point(
                screen,
                mouse_screen,
                zoom_factor(
                    mouse_wheel().1,
                    is_key_down(KeyCode::Q),
                    is_key_down(KeyCode::E),
                ),
            );
        }

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

        user_location.refresh();
        draw_user_location_marker(
            &space,
            screen_width(),
            screen_height(),
            user_location.location,
        );
        if let Some(coord) = app_ui.route.start {
            draw_route_coordinate_marker(&space, coord, GREEN);
        }
        if let Some(coord) = app_ui.route.end {
            draw_route_coordinate_marker(&space, coord, ORANGE);
        }
        if let Some(chooser) = app_ui.tap_chooser {
            draw_route_coordinate_marker(&space, chooser.coord, RED);
        }

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

fn draw_route_coordinate_marker(space: &space::Space, coord: GpsCoord, color: Color) {
    let screen = space.world_to_map_screen(coord, screen_width(), screen_height());
    let x = screen.x as f32;
    let y = screen.y as f32;
    draw_circle(x, y, 13.0, Color::new(color.r, color.g, color.b, 0.25));
    draw_circle(x, y, 7.0, color);
    draw_circle_lines(x, y, 7.0, 2.0, WHITE);
}

fn draw_user_location_marker(
    space: &space::Space,
    screen_width: f32,
    screen_height: f32,
    location: Option<GpsCoord>,
) {
    let Some(location) = location else {
        return;
    };

    let screen = space.world_to_map_screen(location, screen_width, screen_height);
    let x = screen.x as f32;
    let y = screen.y as f32;
    draw_circle(x, y, 13.0, Color::new(0.05, 0.45, 1.0, 0.25));
    draw_circle(x, y, 7.0, Color::new(0.05, 0.45, 1.0, 0.95));
    draw_circle_lines(x, y, 7.0, 2.0, WHITE);
}

fn draw_debug_overlays(
    touches: &[Touch],
    gesture: Option<&TouchGesture>,
    frame_time_ms: f32,
    max_frame_time_ms: f32,
    render_cache_stats: RenderCacheStats,
) {
    let touch_lines = touches.len().min(4);
    let line_count = 5 + touch_lines;
    let padding = 12.0;
    let line_height = DEBUG_LINE_HEIGHT_PX;
    let width = 1040.0;
    let height = padding * 2.0 + line_height * line_count as f32;

    draw_rectangle(8.0, 8.0, width, height, Color::new(0.0, 0.0, 0.0, 0.65));

    let mut y = 8.0 + padding + 28.0;
    draw_text(
        &format!(
            "frame: {:.1} ms ({:.2} fps), max 1s: {:.1} ms",
            frame_time_ms,
            1000.0 / frame_time_ms.max(0.001),
            max_frame_time_ms,
        ),
        16.0,
        y,
        DEBUG_TEXT_SIZE_PX,
        WHITE,
    );

    y += line_height;
    draw_text(
        &format!("touches: {}", touches.len()),
        16.0,
        y,
        DEBUG_TEXT_SIZE_PX,
        WHITE,
    );

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
        DEBUG_TEXT_SIZE_PX,
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
        DEBUG_TEXT_SIZE_PX,
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
            DEBUG_TEXT_SIZE_PX,
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
