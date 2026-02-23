use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use ts_rs::TS;

use crate::rmdf::generator::manifest::TileManifest;
use crate::rmdf::format::TagSetRecord;
use crate::rmdf::io::MappedTile;

/// Response for GET /api/manifest
#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct ManifestResponse {
    pub version: String,
    pub tile_size_degrees: f32,
    pub format_version: u32,
    pub generated_at: String,
    pub tiles: Vec<TileSummary>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct TileSummary {
    pub filename: String,
    pub col: u16,
    pub row: u16,
    pub bounds: TileBoundsResponse,
    pub size_bytes: u64,
    pub point_count: u64,
    pub line_count: u64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct TileBoundsResponse {
    pub lat_min: f32,
    pub lat_max: f32,
    pub lon_min: f32,
    pub lon_max: f32,
}

/// Response for GET /api/tiles/:filename
#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct TileResponse {
    pub filename: String,
    pub header: TileHeader,
    pub points: Vec<PointResponse>,
    pub lines: Vec<LineResponse>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct TileHeader {
    pub point_count: u64,
    pub line_count: u64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct PointResponse {
    pub osm_id: u64,
    pub lat: f32,
    pub lon: f32,
    pub flags: Vec<String>,
    pub connected_lines_count: u32,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct LineResponse {
    pub point_a_osm_id: u64,
    pub point_b_osm_id: u64,
    pub point_a: [f32; 2],  // [lat, lon]
    pub point_b: [f32; 2],  // [lat, lon]
    pub direction: String,
    pub tags: TagResponse,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "ui/src/types/generated")]
pub struct TagResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub highway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub smoothness: Option<String>,
}

impl Default for TagResponse {
    fn default() -> Self {
        Self {
            name: None,
            highway: None,
            surface: None,
            smoothness: None,
        }
    }
}

pub fn get_manifest(input_dir: &PathBuf) -> Result<ManifestResponse> {
    let manifest_path = input_dir.join("manifest.json");
    
    let file = std::fs::File::open(&manifest_path)
        .with_context(|| format!("Failed to open manifest at {:?}", manifest_path))?;
    
    let manifest: TileManifest = serde_json::from_reader(file)
        .context("Failed to parse manifest.json")?;
    
    let response = ManifestResponse {
        version: manifest.version,
        tile_size_degrees: manifest.tile_size_degrees,
        format_version: manifest.format_version,
        generated_at: manifest.generated_at,
        tiles: manifest.tiles.into_iter().map(|t| TileSummary {
            filename: t.filename,
            col: t.col,
            row: t.row,
            bounds: TileBoundsResponse {
                lat_min: t.bounds.lat_min,
                lat_max: t.bounds.lat_max,
                lon_min: t.bounds.lon_min,
                lon_max: t.bounds.lon_max,
            },
            size_bytes: t.size_bytes,
            point_count: t.point_count,
            line_count: t.line_count,
        }).collect(),
    };
    
    Ok(response)
}

pub fn get_tile(input_dir: &PathBuf, filename: &str) -> Result<TileResponse> {
    // Security: Validate filename doesn't contain path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        anyhow::bail!("Invalid filename");
    }
    
    // Security: Must be .rmdf file
    if !filename.ends_with(".rmdf") {
        anyhow::bail!("Filename must end with .rmdf");
    }
    
    let tile_path = input_dir.join(filename);
    let mapped_tile = MappedTile::load(&tile_path)
        .with_context(|| format!("Failed to load tile: {:?}", tile_path))?;
    
    let header = mapped_tile.header;
    
    // Build points response
    let points: Vec<PointResponse> = mapped_tile
        .get_points()?
        .iter()
        .map(|p| {
            let mut flags = Vec::new();
            if p.residential_in_proximity() {
                flags.push("residential_in_proximity".to_string());
            }
            if p.nogo_area() {
                flags.push("nogo_area".to_string());
            }
            PointResponse {
                osm_id: p.osm_id,
                lat: p.lat,
                lon: p.lon,
                flags,
                connected_lines_count: p.lines_count,
            }
        })
        .collect();
    
    // Build lines response
    let lines: Vec<LineResponse> = mapped_tile
        .get_lines()?
        .iter()
        .map(|l| {
            let direction = match l.direction {
                0 => "BothWays",
                1 => "OneWay",
                2 => "Roundabout",
                _ => "Unknown",
            };
            
            // Resolve tags
            let tags = resolve_tags(&mapped_tile, l.tag_set_index).unwrap_or_default();
            
            LineResponse {
                point_a_osm_id: l.point_a_osm_id,
                point_b_osm_id: l.point_b_osm_id,
                point_a: [l.point_a_lat, l.point_a_lon],
                point_b: [l.point_b_lat, l.point_b_lon],
                direction: direction.to_string(),
                tags,
            }
        })
        .collect();
    
    Ok(TileResponse {
        filename: filename.to_string(),
        header: TileHeader {
            point_count: header.point_count,
            line_count: header.line_count,
        },
        points,
        lines,
    })
}

fn resolve_tags(tile: &MappedTile, tag_set_index: u32) -> Result<TagResponse> {
    let tag_set = tile.get_tag_set(tag_set_index)?;
    
    let resolve = |idx: u32| -> Option<String> {
        if idx == TagSetRecord::NONE {
            return None;
        }
        tile.get_tag_value(idx).ok().map(|s| s.to_string())
    };
    
    Ok(TagResponse {
        name: resolve(tag_set.name_idx),
        highway: resolve(tag_set.highway_idx),
        surface: resolve(tag_set.surface_idx),
        smoothness: resolve(tag_set.smoothness_idx),
    })
}
