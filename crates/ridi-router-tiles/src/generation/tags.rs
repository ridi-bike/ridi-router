use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::rmdf::format::{TagSetRecord, TileId};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct TagValueId {
    pub tile_id: TileId,
    pub tag_value_idx: u32,
}

impl TagValueId {
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

    #[allow(dead_code)]
    pub fn from_idx(tile_id: TileId, tag_idx: u32) -> Self {
        if tag_idx == TagSetRecord::NONE {
            Self::none(tile_id)
        } else {
            Self::some(tile_id, tag_idx)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSetId {
    pub tile_id: TileId,
    pub tag_set_idx: u32,
}

impl TagSetId {
    pub fn new(tile_id: TileId, idx: u32) -> Self {
        Self {
            tile_id,
            tag_set_idx: idx,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct TagSet {
    pub name: TagValueId,
    pub hw_ref: TagValueId,
    pub highway: TagValueId,
    pub surface: TagValueId,
    pub smoothness: TagValueId,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GenerationTags {
    pub tag_values: Vec<smartstring::alias::String>,
    pub tag_sets: Vec<TagSet>,
    tag_map: HashMap<smartstring::alias::String, u32>,
    tag_set_map: HashMap<TagSet, u32>,
}

impl GenerationTags {
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
    ) -> TagSetId {
        let placeholder_tile = TileId { col: 0, row: 0 };

        let tag_set = TagSet {
            name: self.get_tag_value_id(name, placeholder_tile),
            hw_ref: self.get_tag_value_id(hw_ref, placeholder_tile),
            highway: self.get_tag_value_id(highway, placeholder_tile),
            surface: self.get_tag_value_id(surface, placeholder_tile),
            smoothness: self.get_tag_value_id(smoothness, placeholder_tile),
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

        TagSetId::new(placeholder_tile, idx)
    }

    fn get_tag_value_id(&mut self, value: Option<&String>, tile_id: TileId) -> TagValueId {
        match value {
            None => TagValueId::none(tile_id),
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
                TagValueId::some(tile_id, idx)
            }
        }
    }
}
