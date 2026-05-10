use std::collections::{HashMap, HashSet};

use crate::decoded_mvt_tile::DecodedMvtTile;
use crate::pmtiles_source::PmtilesSource;
use crate::space::{GpsCoord, WorldBounds};
use crate::tile_address::TileAddress;
use crate::web_mercator_tiles::{lon_lat_to_tile, zoom_for_lon_span};
use tracing::{debug, error, info, trace, warn};

pub struct VisibleTileDownloader {
    source: PmtilesSource,
    runtime: tokio::runtime::Runtime,
    tiles: HashMap<TileAddress, DecodedMvtTile>,
    failed: HashSet<TileAddress>,
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
        })
    }

    pub fn update_visible_tiles(&mut self, bounds: WorldBounds, max_zoom: u8) {
        let addresses = visible_tile_addresses(bounds, max_zoom);
        debug!(
            min_lat = bounds.min.lat,
            min_lon = bounds.min.lon,
            max_lat = bounds.max.lat,
            max_lon = bounds.max.lon,
            tile_count = addresses.len(),
            "updating visible tiles"
        );
        for address in addresses {
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

    pub fn visible_tiles(
        &self,
        bounds: WorldBounds,
        max_zoom: u8,
    ) -> impl Iterator<Item = &DecodedMvtTile> {
        let visible: HashSet<_> = visible_tile_addresses(bounds, max_zoom)
            .into_iter()
            .collect();
        self.tiles
            .iter()
            .filter(move |(address, _tile)| visible.contains(address))
            .map(|(_address, tile)| tile)
    }
}

pub fn visible_tile_addresses(bounds: WorldBounds, max_zoom: u8) -> Vec<TileAddress> {
    let zoom = zoom_for_lon_span(bounds.max.lon - bounds.min.lon, max_zoom);
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

    let mut addresses = Vec::new();
    for y in north_west.y.min(south_east.y)..=north_west.y.max(south_east.y) {
        for x in north_west.x.min(south_east.x)..=north_west.x.max(south_east.x) {
            addresses.push(TileAddress::new(zoom, x, y));
        }
    }

    addresses
}
