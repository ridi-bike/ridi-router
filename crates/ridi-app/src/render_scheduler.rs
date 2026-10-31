use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;

use crate::render_worker::WorkerFreshness;
use crate::retained_renderer::{
    EveryTenthGenerationRetryPolicy, TileBuildState, TileFailure, TileKey, TileRetryPolicy,
    WorkGeneration, WorkRequest,
};
use crate::space::{GpsCoord, WorldBounds};
use crate::tile_address::TileAddress;
use crate::web_mercator_tiles::lon_lat_to_tile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleTileRequest {
    pub zoom: u8,
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

impl VisibleTileRequest {
    pub fn addresses(self) -> Vec<TileAddress> {
        let mut addresses = Vec::new();
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                addresses.push(TileAddress::new(self.zoom, x, y));
            }
        }
        addresses
    }
}

pub struct RenderScheduler {
    work_tx: SyncSender<WorkRequest>,
    freshness: Arc<WorkerFreshness>,
    style_revision: u64,
    generation: WorkGeneration,
    last_request: Option<VisibleTileRequest>,
    pending: VecDeque<TileAddress>,
    states: HashMap<TileKey, TileBuildState>,
    failed: HashMap<TileAddress, TileFailure>,
    retry_policy: EveryTenthGenerationRetryPolicy,
    visible: Vec<TileAddress>,
    previous_zoom_visible: Vec<TileAddress>,
}

impl RenderScheduler {
    pub fn new(
        work_tx: SyncSender<WorkRequest>,
        freshness: Arc<WorkerFreshness>,
        style_revision: u64,
    ) -> Self {
        Self {
            work_tx,
            freshness,
            style_revision,
            generation: WorkGeneration(0),
            last_request: None,
            pending: VecDeque::new(),
            states: HashMap::new(),
            failed: HashMap::new(),
            retry_policy: EveryTenthGenerationRetryPolicy,
            visible: Vec::new(),
            previous_zoom_visible: Vec::new(),
        }
    }

    pub fn request_visible_tiles(&mut self, bounds: WorldBounds, zoom: u8) {
        let request = visible_tile_request(bounds, zoom);
        if self.last_request != Some(request) {
            if !self.visible.is_empty() && self.visible.first().map(|tile| tile.z) != Some(zoom) {
                self.previous_zoom_visible = self.visible.clone();
            }
            self.generation.0 = self.generation.0.wrapping_add(1);
            self.clear_queued_states();
            self.last_request = Some(request);
            self.visible = request.addresses();
            let active_tiles = self.enqueue_priority_work(request);
            self.retry_failed_visible();
            self.prune_pending_to(&active_tiles);
            self.freshness.update_request(self.generation, active_tiles);
        } else {
            self.retry_failed_visible();
        }
        self.flush_pending(32);
    }

    pub fn visible_tiles(&self) -> &[TileAddress] {
        &self.visible
    }

    pub fn fallback_tiles(&self) -> &[TileAddress] {
        &self.previous_zoom_visible
    }

    pub fn generation(&self) -> WorkGeneration {
        self.generation
    }

    pub fn style_revision(&self) -> u64 {
        self.style_revision
    }

    pub fn mark_partial(&mut self, tile_key: TileKey) {
        self.states.insert(tile_key, TileBuildState::Partial);
        self.failed.remove(&tile_key.address);
    }

    pub fn mark_complete(&mut self, tile_key: TileKey) {
        self.states.insert(tile_key, TileBuildState::Complete);
        self.failed.remove(&tile_key.address);
    }

    pub fn mark_missing(&mut self, tile_key: TileKey) {
        self.states.insert(tile_key, TileBuildState::Failed);
        self.failed.insert(tile_key.address, TileFailure::Missing);
    }

    pub fn mark_failed(&mut self, tile_key: TileKey) {
        self.states.insert(tile_key, TileBuildState::Failed);
        self.failed.insert(tile_key.address, TileFailure::Failed);
    }

    fn enqueue_priority_work(&mut self, request: VisibleTileRequest) -> HashSet<TileAddress> {
        let mut seen = HashSet::new();
        let center = visible_center(&self.visible);
        let mut visible = self.visible.clone();
        visible.sort_by_key(|tile| {
            let dx = tile.x as i64 - center.0 as i64;
            let dy = tile.y as i64 - center.1 as i64;
            dx * dx + dy * dy
        });

        // 1. Current visible tiles.
        for tile in visible {
            self.enqueue_once(tile, &mut seen);
        }

        // 2. Edge tiles likely to enter during pan.
        let max_tile = (1_u32 << request.zoom).saturating_sub(1);
        let min_x = request.min_x.saturating_sub(1);
        let max_x = (request.max_x + 1).min(max_tile);
        let min_y = request.min_y.saturating_sub(1);
        let max_y = (request.max_y + 1).min(max_tile);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if x < request.min_x || x > request.max_x || y < request.min_y || y > request.max_y
                {
                    self.enqueue_once(TileAddress::new(request.zoom, x, y), &mut seen);
                }
            }
        }

        // 3. Parent fallback tiles for zoom transitions.
        for tile in self.visible.clone() {
            if tile.z > 0 {
                self.enqueue_once(
                    TileAddress::new(tile.z - 1, tile.x / 2, tile.y / 2),
                    &mut seen,
                );
            }
        }

        // 4. Adjacent zoom prewarm.
        if request.zoom > 0 {
            let parent_request = VisibleTileRequest {
                zoom: request.zoom - 1,
                min_x: request.min_x / 2,
                max_x: request.max_x / 2,
                min_y: request.min_y / 2,
                max_y: request.max_y / 2,
            };
            for tile in parent_request.addresses() {
                self.enqueue_once(tile, &mut seen);
            }
        }
        if request.zoom < 15 {
            for tile in self.visible.clone() {
                let child_z = tile.z + 1;
                for child_y in tile.y * 2..=tile.y * 2 + 1 {
                    for child_x in tile.x * 2..=tile.x * 2 + 1 {
                        self.enqueue_once(TileAddress::new(child_z, child_x, child_y), &mut seen);
                    }
                }
            }
        }
        seen
    }

    fn enqueue_once(&mut self, tile: TileAddress, seen: &mut HashSet<TileAddress>) {
        if seen.insert(tile) && self.should_enqueue(tile) {
            self.pending.push_back(tile);
        }
    }

    fn retry_failed_visible(&mut self) {
        if self.generation.0 % 10 != 0 {
            return;
        }
        for tile in self.visible.clone() {
            let Some(failure) = self.failed.get(&tile).copied() else {
                continue;
            };
            if self
                .retry_policy
                .should_retry(tile, self.generation, failure)
            {
                self.pending.push_back(tile);
            }
        }
    }

    fn clear_queued_states(&mut self) {
        self.states.retain(|key, state| {
            key.style_revision != self.style_revision
                || !matches!(
                    *state,
                    TileBuildState::Queued | TileBuildState::Building | TileBuildState::Partial
                )
        });
    }

    fn prune_pending_to(&mut self, active_tiles: &HashSet<TileAddress>) {
        self.pending.retain(|tile| active_tiles.contains(tile));
    }

    fn should_enqueue(&self, tile: TileAddress) -> bool {
        let key = TileKey {
            address: tile,
            style_revision: self.style_revision,
        };
        !matches!(
            self.states.get(&key),
            Some(
                TileBuildState::Queued
                    | TileBuildState::Building
                    | TileBuildState::Partial
                    | TileBuildState::Complete
            )
        )
    }

    fn flush_pending(&mut self, max_per_frame: usize) {
        for _ in 0..max_per_frame {
            let Some(tile) = self.pending.pop_front() else {
                return;
            };
            if !self.should_enqueue(tile) {
                continue;
            }
            let request = WorkRequest::BuildTile {
                tile,
                style_revision: self.style_revision,
                generation: self.generation,
            };
            match self.work_tx.try_send(request) {
                Ok(()) => {
                    self.states.insert(
                        TileKey {
                            address: tile,
                            style_revision: self.style_revision,
                        },
                        TileBuildState::Queued,
                    );
                }
                Err(TrySendError::Full(request)) => {
                    let WorkRequest::BuildTile { tile, .. } = request;
                    self.pending.push_front(tile);
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }
}

pub fn visible_tile_request(bounds: WorldBounds, zoom: u8) -> VisibleTileRequest {
    let north_west = lon_lat_to_tile(
        GpsCoord {
            lat: bounds.max.lat,
            lon: bounds.min.lon,
        },
        zoom,
    );
    let south_east = lon_lat_to_tile(
        GpsCoord {
            lat: bounds.min.lat,
            lon: bounds.max.lon,
        },
        zoom,
    );

    VisibleTileRequest {
        zoom,
        min_x: north_west.x.min(south_east.x),
        max_x: north_west.x.max(south_east.x),
        min_y: north_west.y.min(south_east.y),
        max_y: north_west.y.max(south_east.y),
    }
}

fn visible_center(tiles: &[TileAddress]) -> (u32, u32) {
    if tiles.is_empty() {
        return (0, 0);
    }
    let x = tiles.iter().map(|tile| tile.x as u64).sum::<u64>() / tiles.len() as u64;
    let y = tiles.iter().map(|tile| tile.y as u64).sum::<u64>() / tiles.len() as u64;
    (x as u32, y as u32)
}
