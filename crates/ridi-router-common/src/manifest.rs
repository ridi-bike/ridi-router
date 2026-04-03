use serde::{Deserialize, Serialize};

use crate::format::TileId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileManifest {
    pub version: String,
    pub tile_size_degrees: f32,
    pub format_version: u32,
    pub generated_at: String,
    pub source_files: Vec<String>,
    pub tiles: Vec<TileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileMetadata {
    pub filename: String,
    pub col: u16,
    pub row: u16,
    pub bounds: TileBounds,
    pub neighbors: TileNeighbors,
    pub size_bytes: u64,
    pub point_count: u64,
    pub line_count: u64,
    pub checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub military_geojson_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileBounds {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileNeighbors {
    pub north: Option<String>,
    pub south: Option<String>,
    pub east: Option<String>,
    pub west: Option<String>,
    pub northeast: Option<String>,
    pub northwest: Option<String>,
    pub southeast: Option<String>,
    pub southwest: Option<String>,
}

pub fn military_geojson_filename(tile_id: TileId) -> String {
    format!("tile_{}_{}.military.geojson", tile_id.col, tile_id.row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_types_round_trip_serialize_deserialize() {
        let manifest = TileManifest {
            version: "0.8.6".to_string(),
            tile_size_degrees: 1.0,
            format_version: 1,
            generated_at: "2026-04-03T00:00:00Z".to_string(),
            source_files: vec!["input.osm.pbf".to_string()],
            tiles: vec![TileMetadata {
                filename: "tile_200_100.rmdf".to_string(),
                col: 200,
                row: 100,
                bounds: TileBounds {
                    lat_min: 10.0,
                    lat_max: 11.0,
                    lon_min: 20.0,
                    lon_max: 21.0,
                },
                neighbors: TileNeighbors {
                    north: None,
                    south: None,
                    east: None,
                    west: None,
                    northeast: None,
                    northwest: None,
                    southeast: None,
                    southwest: None,
                },
                size_bytes: 123,
                point_count: 10,
                line_count: 9,
                checksum: "sha256:test".to_string(),
                military_geojson_filename: Some(military_geojson_filename(TileId {
                    col: 200,
                    row: 100,
                })),
            }],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let round_trip: TileManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip, manifest);
    }

    #[test]
    fn manifest_filename_helper_matches_expected_value() {
        assert_eq!(
            military_geojson_filename(TileId { col: 12, row: 34 }),
            "tile_12_34.military.geojson"
        );
    }

    #[test]
    fn shared_manifest_schema_is_data_only() {
        let source = include_str!("manifest.rs");
        let schema_only = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(!schema_only.contains("clap::"));
        assert!(!schema_only.contains("anyhow::"));
    }
}
