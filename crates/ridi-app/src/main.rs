pub mod decoded_mvt_tile;
mod drag;
mod keyboard_pan;
mod map_rendering_spec;
mod mvt_geometry;
pub mod pmtiles_source;
mod screen_layout;
mod space;
pub mod tile_address;
mod tile_renderer;
mod viewport;
mod visible_tiles;
pub mod web_mercator_tiles;
mod zoom;

use drag::DragState;
use keyboard_pan::KeyboardPan;
use macroquad::prelude::*;
use map_rendering_spec::MapRenderingSpec;
use screen_layout::padded_screen_rect;
use space::{GpsCoord, ScreenCoord};
use tile_renderer::draw_map_tiles;
use tracing::{debug, error, info};
use viewport::Viewport;
use visible_tiles::VisibleTileDownloader;
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
    let mut drag = DragState::default();
    let mut tile_downloader = match VisibleTileDownloader::open_ridi_map() {
        Ok(tile_downloader) => {
            info!("tile downloader initialized");
            Some(tile_downloader)
        }
        Err(error) => {
            error!(%error, "tile downloader initialization failed");
            None
        }
    };

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
        viewport.pan_by_fraction(keyboard_pan.lat_fraction(), keyboard_pan.lon_fraction());

        if let Some(previous_mouse_position) =
            drag.update(is_mouse_button_down(MouseButton::Left), mouse_screen)
        {
            viewport.pan_between_screen_points(screen, previous_mouse_position, mouse_screen);
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
        let bounds = viewport.world_bounds();

        if let Some(tile_downloader) = &mut tile_downloader {
            debug!("updating and drawing visible tiles");
            tile_downloader.update_visible_tiles(bounds, MAX_TILE_ZOOM);
            draw_map_tiles(
                tile_downloader.visible_tiles(bounds, MAX_TILE_ZOOM),
                &space,
                &rendering_spec,
                label_font.as_ref(),
            );
        }

        next_frame().await;
    }
}
