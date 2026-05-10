use anyhow::{Context, Result};
use pmtiles::reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use pmtiles::reqwest::Client;
use pmtiles::{AsyncPmTilesReader, HashMapCache, TileCoord};
use tracing::{debug, error, info, warn};

use crate::tile_address::TileAddress;

pub const RIDI_MAP_PMTILES_URL: &str = "https://maps.ridi.bike/map.pmtiles";

pub struct PmtilesSource {
    reader: AsyncPmTilesReader<pmtiles::HttpBackend, HashMapCache>,
}

impl PmtilesSource {
    pub async fn open(url: &str) -> Result<Self> {
        info!(url, "opening PMTiles source");
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("ridi-app/0.8.6 (+https://ridi.bike)"),
        );

        let client = Client::builder()
            .use_rustls_tls()
            .default_headers(headers)
            .build()
            .context("building PMTiles HTTP client")?;
        debug!("built PMTiles HTTP client");
        let cache = HashMapCache::default();
        let reader = AsyncPmTilesReader::new_with_cached_url(cache, client, url)
            .await
            .context("opening PMTiles archive")?;
        info!(url, "opened PMTiles archive");

        Ok(Self { reader })
    }

    pub async fn open_ridi_map() -> Result<Self> {
        Self::open(RIDI_MAP_PMTILES_URL).await
    }

    pub async fn fetch_tile(&self, address: TileAddress) -> Result<Option<Vec<u8>>> {
        debug!(
            z = address.z,
            x = address.x,
            y = address.y,
            "requesting PMTiles tile"
        );
        let coord =
            TileCoord::new(address.z, address.x, address.y).context("invalid tile address")?;
        let tile = self.reader.get_tile_decompressed(coord).await;

        match tile {
            Ok(Some(bytes)) => {
                debug!(
                    z = address.z,
                    x = address.x,
                    y = address.y,
                    byte_len = bytes.len(),
                    "received PMTiles tile"
                );
                Ok(Some(bytes.to_vec()))
            }
            Ok(None) => {
                warn!(
                    z = address.z,
                    x = address.x,
                    y = address.y,
                    "PMTiles tile not found"
                );
                Ok(None)
            }
            Err(error) => {
                error!(z = address.z, x = address.x, y = address.y, %error, "PMTiles tile request failed");
                Err(error).context("fetching PMTiles tile")
            }
        }
    }
}
