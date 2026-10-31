use std::borrow::Cow;

use anyhow::{Context, Result};
use geozero::mvt::{tile, Message, Tile};
use geozero::{FeatureProcessor, GeomProcessor, GeozeroDatasource, GeozeroGeometry};
use tracing::debug;

use crate::tile_address::TileAddress;

#[derive(Debug, Clone)]
pub struct DecodedMvtTile {
    pub address: TileAddress,
    tile: Tile,
}

impl DecodedMvtTile {
    pub fn decode(address: TileAddress, bytes: &[u8]) -> Result<Self> {
        let tile = Tile::decode(bytes).context("decoding MVT tile")?;
        debug!(
            z = address.z,
            x = address.x,
            y = address.y,
            layer_count = tile.layers.len(),
            feature_count = tile
                .layers
                .iter()
                .map(|layer| layer.features.len())
                .sum::<usize>(),
            "decoded MVT tile bytes"
        );
        Ok(Self { address, tile })
    }

    pub fn layer_count(&self) -> usize {
        self.tile.layers.len()
    }

    pub fn feature_count(&self) -> usize {
        self.tile
            .layers
            .iter()
            .map(|layer| layer.features.len())
            .sum()
    }

    pub fn layers(&self) -> impl Iterator<Item = MvtLayer<'_>> {
        self.tile
            .layers
            .iter()
            .enumerate()
            .map(|(layer_index, layer)| MvtLayer { layer, layer_index })
    }

    pub fn features(&self) -> impl Iterator<Item = MvtFeature<'_>> {
        self.layers().flat_map(|layer| layer.features())
    }

    pub fn features_in_layer<'a>(
        &'a self,
        layer_name: &'a str,
    ) -> impl Iterator<Item = MvtFeature<'a>> {
        self.layers()
            .filter(move |layer| layer.name() == layer_name)
            .flat_map(|layer| layer.features())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MvtLayer<'a> {
    layer: &'a tile::Layer,
    layer_index: usize,
}

impl<'a> MvtLayer<'a> {
    pub fn name(self) -> &'a str {
        &self.layer.name
    }

    pub fn extent(self) -> u32 {
        self.layer.extent.unwrap_or(4096)
    }

    pub fn layer_index(self) -> usize {
        self.layer_index
    }

    pub fn features(self) -> impl Iterator<Item = MvtFeature<'a>> {
        self.layer
            .features
            .iter()
            .enumerate()
            .map(move |(feature_index, feature)| MvtFeature {
                layer: self.layer,
                layer_index: self.layer_index,
                feature,
                feature_index,
            })
    }

    pub fn process<P: FeatureProcessor>(self, processor: &mut P) -> geozero::error::Result<()> {
        let mut layer = self.layer.clone();
        layer.process(processor)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MvtFeature<'a> {
    layer: &'a tile::Layer,
    layer_index: usize,
    feature: &'a tile::Feature,
    feature_index: usize,
}

impl<'a> MvtFeature<'a> {
    pub fn layer_name(self) -> &'a str {
        &self.layer.name
    }

    pub fn id(self) -> Option<u64> {
        self.feature.id
    }

    pub fn geometry_type(self) -> Option<tile::GeomType> {
        self.feature.r#type.and_then(tile::GeomType::from_i32)
    }

    pub fn layer_index(self) -> usize {
        self.layer_index
    }

    pub fn feature_index(self) -> usize {
        self.feature_index
    }

    pub fn extent(self) -> u32 {
        self.layer.extent.unwrap_or(4096)
    }

    pub fn properties(self) -> impl Iterator<Item = MvtProperty<'a>> {
        self.feature.tags.chunks_exact(2).filter_map(move |tag| {
            let key = self.layer.keys.get(tag[0] as usize)?;
            let value = self.layer.values.get(tag[1] as usize)?;
            Some(MvtProperty { key, value })
        })
    }

    pub fn process_geometry<P: GeomProcessor>(
        self,
        processor: &mut P,
    ) -> geozero::error::Result<()> {
        self.feature.process_geom(processor)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MvtProperty<'a> {
    key: &'a str,
    value: &'a tile::Value,
}

impl<'a> MvtProperty<'a> {
    pub fn key(self) -> &'a str {
        self.key
    }

    pub fn value(self) -> MvtValue<'a> {
        MvtValue::from(self.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MvtValue<'a> {
    String(&'a str),
    Float(f32),
    Double(f64),
    Int(i64),
    UInt(u64),
    SInt(i64),
    Bool(bool),
    Empty,
}

impl<'a> MvtValue<'a> {
    pub fn as_str(self) -> Option<&'a str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

impl<'a> From<&'a tile::Value> for MvtValue<'a> {
    fn from(value: &'a tile::Value) -> Self {
        if let Some(value) = value.string_value.as_deref() {
            Self::String(value)
        } else if let Some(value) = value.float_value {
            Self::Float(value)
        } else if let Some(value) = value.double_value {
            Self::Double(value)
        } else if let Some(value) = value.int_value {
            Self::Int(value)
        } else if let Some(value) = value.uint_value {
            Self::UInt(value)
        } else if let Some(value) = value.sint_value {
            Self::SInt(value)
        } else if let Some(value) = value.bool_value {
            Self::Bool(value)
        } else {
            Self::Empty
        }
    }
}

#[derive(Debug, Clone)]
pub struct TileBytes<'a> {
    bytes: Cow<'a, [u8]>,
}

impl<'a> TileBytes<'a> {
    pub fn borrowed(bytes: &'a [u8]) -> Self {
        Self {
            bytes: Cow::Borrowed(bytes),
        }
    }

    pub fn owned(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Cow::Owned(bytes),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}
