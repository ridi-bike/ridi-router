use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use macroquad::prelude::Color;
use serde::Deserialize;
use tracing::warn;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct MapRenderingSpec {
    pub version: u32,
    pub source: String,
    pub defaults: RenderingDefaults,
    pub zoom_ranges: Vec<ZoomRenderingSpec>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RenderingDefaults {
    pub background: String,
    pub text_font: String,
    pub font_paths: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ZoomRenderingSpec {
    pub name: String,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub render_order: Vec<RenderRule>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RenderRule {
    pub id: String,
    pub layer: String,
    pub filter: FeatureFilter,
    pub render: RenderInstruction,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FeatureFilter {
    pub kind: Option<String>,
    pub kind_any: Option<Vec<String>>,
    pub kind_not: Option<String>,
    pub kind_detail: Option<String>,
    pub kind_detail_any: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum RenderInstruction {
    Fill(FillStyle),
    Line(LineStyle),
    Circle(CircleStyle),
    Text(TextStyle),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FillStyle {
    pub color: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct LineStyle {
    pub color: String,
    pub width_px: f32,
    pub dash: Option<Vec<f32>>,
    pub casing_color: Option<String>,
    pub casing_width_px: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircleStyle {
    pub color: String,
    pub radius_px: f32,
    pub stroke_color: Option<String>,
    pub stroke_width_px: Option<f32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct TextStyle {
    pub field: Option<String>,
    pub fields: Option<Vec<String>>,
    pub color: String,
    pub size_px: f32,
    pub weight: String,
    pub transform: Option<TextTransform>,
    pub halo_color: Option<String>,
    pub halo_width_px: Option<f32>,
    pub offset_px: Option<PixelOffset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TextTransform {
    Uppercase,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PixelOffset {
    pub x: f32,
    pub y: f32,
}

impl MapRenderingSpec {
    pub fn load() -> Self {
        match load_from_known_locations() {
            Ok(spec) => spec,
            Err(error) => {
                warn!(%error, "failed to load map rendering spec; using built-in fallback");
                ron::from_str(include_str!("../map-rendering.ron"))
                    .expect("built-in map-rendering.ron must be valid")
            }
        }
    }

    pub async fn load_font(&self) -> Option<macroquad::prelude::Font> {
        for path in &self.defaults.font_paths {
            match macroquad::prelude::load_ttf_font(path).await {
                Ok(font) => return Some(font),
                Err(error) => warn!(%error, path, "failed to load map label font"),
            }
        }

        warn!(
            text_font = %self.defaults.text_font,
            "no configured map label font could be loaded; falling back to macroquad default font"
        );
        None
    }

    pub fn rules_for_zoom(&self, zoom: u8) -> impl Iterator<Item = &RenderRule> {
        self.zoom_ranges
            .iter()
            .find(move |range| zoom >= range.min_zoom && zoom <= range.max_zoom)
            .into_iter()
            .flat_map(|range| range.render_order.iter())
    }

    pub fn background_color(&self) -> Color {
        parse_hex_color(&self.defaults.background).unwrap_or(Color::from_rgba(255, 255, 255, 255))
    }
}

impl FillStyle {
    pub fn color(&self) -> Color {
        parse_hex_color(&self.color).unwrap_or(Color::from_rgba(0, 0, 0, 255))
    }
}

impl LineStyle {
    pub fn color(&self) -> Color {
        parse_hex_color(&self.color).unwrap_or(Color::from_rgba(0, 0, 0, 255))
    }

    pub fn casing_color(&self) -> Option<Color> {
        self.casing_color.as_deref().and_then(parse_hex_color)
    }
}

impl CircleStyle {
    pub fn color(&self) -> Color {
        parse_hex_color(&self.color).unwrap_or(Color::from_rgba(0, 0, 0, 255))
    }

    pub fn stroke_color(&self) -> Option<Color> {
        self.stroke_color.as_deref().and_then(parse_hex_color)
    }
}

impl TextStyle {
    pub fn color(&self) -> Color {
        parse_hex_color(&self.color).unwrap_or(Color::from_rgba(0, 0, 0, 255))
    }

    pub fn halo_color(&self) -> Option<Color> {
        self.halo_color.as_deref().and_then(parse_hex_color)
    }
}

fn load_from_known_locations() -> Result<MapRenderingSpec> {
    let path = known_spec_locations()
        .into_iter()
        .find(|path| path.exists())
        .context("map-rendering.ron was not found in known locations")?;
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    ron::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

fn known_spec_locations() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("map-rendering.ron")];
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        paths.push(Path::new(&manifest_dir).join("map-rendering.ron"));
    }
    paths.push(PathBuf::from("crates/ridi-app/map-rendering.ron"));
    paths
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let (r, g, b, a) = match value.len() {
        6 => (
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
            u8::from_str_radix(&value[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color::from_rgba(r, g, b, a))
}

#[cfg(test)]
mod tests {
    use super::MapRenderingSpec;

    #[test]
    fn bundled_map_rendering_spec_parses() {
        let spec: MapRenderingSpec = ron::from_str(include_str!("../map-rendering.ron"))
            .expect("map-rendering.ron should parse");
        assert_eq!(spec.version, 1);
        assert!(!spec.zoom_ranges.is_empty());
    }
}
