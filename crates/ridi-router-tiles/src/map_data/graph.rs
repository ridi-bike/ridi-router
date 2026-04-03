use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    hash::Hash,
    marker::PhantomData,
};

use serde::{Deserialize, Serialize};

use crate::rmdf::format::{TagSetRecord, TileId};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagValueRef {
    pub tile_id: TileId,
    pub tag_value_idx: u32,
}

impl ElementTagValueRef {
    pub fn none(tile_id: TileId) -> Self {
        Self {
            tile_id,
            tag_value_idx: TagSetRecord::NONE,
        }
    }

    pub fn some(tile_id: TileId, tag_idx: u32) -> Self {
        Self {
            tile_id,
            tag_value_idx: tag_idx,
        }
    }

    pub fn from_idx(tile_id: TileId, tag_idx: u32) -> Self {
        if tag_idx == TagSetRecord::NONE {
            Self::none(tile_id)
        } else {
            Self::some(tile_id, tag_idx)
        }
    }

    pub fn get(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagSetRef {
    pub tile_id: TileId,
    pub tag_set_idx: u32,
}

impl ElementTagSetRef {
    pub fn new(tile_id: TileId, idx: u32) -> Self {
        Self {
            tile_id,
            tag_set_idx: idx,
        }
    }

    pub fn get(&self) -> ElementTagSet {
        ElementTagSet {
            name: ElementTagValueRef::none(self.tile_id),
            hw_ref: ElementTagValueRef::none(self.tile_id),
            highway: ElementTagValueRef::none(self.tile_id),
            surface: ElementTagValueRef::none(self.tile_id),
            smoothness: ElementTagValueRef::none(self.tile_id),
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTagSet {
    pub name: ElementTagValueRef,
    pub hw_ref: ElementTagValueRef,
    pub highway: ElementTagValueRef,
    pub surface: ElementTagValueRef,
    pub smoothness: ElementTagValueRef,
}

impl ElementTagSet {
    pub fn name(&self) -> Option<String> {
        self.name.get()
    }

    pub fn hw_ref(&self) -> Option<String> {
        self.hw_ref.get()
    }

    pub fn highway(&self) -> Option<String> {
        self.highway.get()
    }

    pub fn surface(&self) -> Option<String> {
        self.surface.get()
    }

    pub fn smoothness(&self) -> Option<String> {
        self.smoothness.get()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ElementTags {
    pub tag_values: Vec<smartstring::alias::String>,
    pub tag_sets: Vec<ElementTagSet>,
    tag_map: HashMap<smartstring::alias::String, u32>,
    tag_set_map: HashMap<ElementTagSet, u32>,
}

impl ElementTags {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> (usize, usize) {
        (self.tag_values.len(), self.tag_sets.len())
    }

    #[allow(dead_code)]
    pub fn clear_maps(&mut self) {
        self.tag_set_map = HashMap::new();
        self.tag_map = HashMap::new();
    }

    pub fn get_or_create(
        &mut self,
        name: Option<&String>,
        hw_ref: Option<&String>,
        highway: Option<&String>,
        surface: Option<&String>,
        smoothness: Option<&String>,
    ) -> ElementTagSetRef {
        let placeholder_tile = TileId { col: 0, row: 0 };

        let tag_set = ElementTagSet {
            name: self.get_tag_value_ref(name, placeholder_tile),
            hw_ref: self.get_tag_value_ref(hw_ref, placeholder_tile),
            highway: self.get_tag_value_ref(highway, placeholder_tile),
            surface: self.get_tag_value_ref(surface, placeholder_tile),
            smoothness: self.get_tag_value_ref(smoothness, placeholder_tile),
        };

        let idx = match self.tag_set_map.get(&tag_set) {
            Some(i) => *i,
            None => {
                let new_idx = self.tag_sets.len() as u32;
                self.tag_set_map.insert(tag_set.clone(), new_idx);
                self.tag_sets.push(tag_set);
                new_idx
            }
        };

        ElementTagSetRef::new(placeholder_tile, idx)
    }

    fn get_tag_value_ref(&mut self, value: Option<&String>, tile_id: TileId) -> ElementTagValueRef {
        match value {
            None => ElementTagValueRef::none(tile_id),
            Some(v) => {
                let normalized = if v.ends_with("_link") {
                    v.replace("_link", "")
                } else {
                    v.to_string()
                };
                let key = smartstring::alias::String::from(&normalized);
                let idx = match self.tag_map.get(&key) {
                    Some(i) => *i,
                    None => {
                        let new_idx = self.tag_values.len() as u32;
                        self.tag_values.push(key.clone());
                        self.tag_map.insert(key, new_idx);
                        new_idx
                    }
                };
                ElementTagValueRef::some(tile_id, idx)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct MapDataElementRef<T> {
    tile_id: TileId,
    element_id: u64,
    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T> MapDataElementRef<T> {
    pub fn new(tile_id: TileId, element_id: u64) -> Self {
        Self {
            tile_id,
            element_id,
            _marker: PhantomData,
        }
    }

    pub fn get_tile_id(&self) -> TileId {
        self.tile_id
    }

    pub fn get_element_id(&self) -> u64 {
        self.element_id
    }

    pub fn get(&self) -> T {
        panic!("tile-generation refs are not dereferenceable")
    }
}

impl<T> Display for MapDataElementRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ref(tile:{:?}, id:{})", self.tile_id, self.element_id)
    }
}

pub type MapDataPointRef = MapDataElementRef<super::point::MapDataPoint>;
pub type MapDataLineRef = MapDataElementRef<super::line::MapDataLine>;
