use std::collections::{HashMap, HashSet};

use crate::decoded_mvt_tile::DecodedMvtTile;
use crate::pmtiles_source::PmtilesSource;
use crate::space::{GpsCoord, WorldBounds};
use crate::tile_address::TileAddress;
use crate::web_mercator_tiles::lon_lat_to_tile;
use tracing::{debug, error, info, trace, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VisibleTileRequest {
    zoom: u8,
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
}

impl VisibleTileRequest {
    fn addresses(self) -> Vec<TileAddress> {
        let mut addresses = Vec::new();
        for y in self.min_y..=self.max_y {
            for x in self.min_x..=self.max_x {
                addresses.push(TileAddress::new(self.zoom, x, y));
            }
        }
        addresses
    }
}

pub struct VisibleTileDownloader {
    source: PmtilesSource,
    runtime: tokio::runtime::Runtime,
    tiles: HashMap<TileAddress, DecodedMvtTile>,
    failed: HashSet<TileAddress>,
    visible_request: Option<VisibleTileRequest>,
    visible_addresses: Vec<TileAddress>,
}

impl VisibleTileDownloader {
    pub fn open_ridi_map() -> anyhow::Result<Self> {
        info!("opening visible tile downloader");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let source = runtime.block_on(PmtilesSource::open_ridi_map())?;
        info!("visible tile downloader ready");

        Ok(Self {
            source,
            runtime,
            tiles: HashMap::new(),
            failed: HashSet::new(),
            visible_request: None,
            visible_addresses: Vec::new(),
        })
    }

    pub fn update_visible_tiles(&mut self, bounds: WorldBounds, zoom: u8) {
        let request = visible_tile_request(bounds, zoom);
        if self.visible_request == Some(request) {
            return;
        }

        self.visible_addresses = request.addresses();
        self.visible_request = Some(request);

        debug!(
            min_lat = bounds.min.lat,
            min_lon = bounds.min.lon,
            max_lat = bounds.max.lat,
            max_lon = bounds.max.lon,
            tile_count = self.visible_addresses.len(),
            "updating visible tiles"
        );
        for &address in &self.visible_addresses {
            if self.tiles.contains_key(&address) {
                trace!(
                    z = address.z,
                    x = address.x,
                    y = address.y,
                    "visible tile already cached"
                );
                continue;
            }
            if self.failed.contains(&address) {
                trace!(
                    z = address.z,
                    x = address.x,
                    y = address.y,
                    "visible tile previously failed"
                );
                continue;
            }

            info!(
                z = address.z,
                x = address.x,
                y = address.y,
                "downloading visible tile"
            );
            let tile = self.runtime.block_on(self.source.fetch_tile(address));
            match tile {
                Ok(Some(bytes)) => {
                    debug!(
                        z = address.z,
                        x = address.x,
                        y = address.y,
                        byte_len = bytes.len(),
                        "decoding visible tile"
                    );
                    match DecodedMvtTile::decode(address, &bytes) {
                        Ok(tile) => {
                            info!(
                                z = address.z,
                                x = address.x,
                                y = address.y,
                                layer_count = tile.layer_count(),
                                feature_count = tile.feature_count(),
                                "decoded visible tile"
                            );
                            self.tiles.insert(address, tile);
                        }
                        Err(error) => {
                            error!(z = address.z, x = address.x, y = address.y, %error, "failed to decode visible tile");
                            self.failed.insert(address);
                        }
                    }
                }
                Ok(None) => {
                    warn!(
                        z = address.z,
                        x = address.x,
                        y = address.y,
                        "visible tile missing from PMTiles"
                    );
                    self.failed.insert(address);
                }
                Err(error) => {
                    error!(z = address.z, x = address.x, y = address.y, %error, "failed to download visible tile");
                    self.failed.insert(address);
                }
            }
        }
    }

    pub fn visible_tiles(&self) -> impl Iterator<Item = &DecodedMvtTile> {
        self.visible_addresses
            .iter()
            .filter_map(|address| self.tiles.get(address))
    }
}

fn visible_tile_request(bounds: WorldBounds, zoom: u8) -> VisibleTileRequest {
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
