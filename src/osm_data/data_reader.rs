use crate::map_data::graph::MapDataGraph;

use super::{json_reader::JsonReader, pbf_reader::PbfReader, DataSource, OsmDataReaderError};

pub const ALLOWED_ACCESS_VALUES: [&str; 3] = ["yes", "permissive", "public"];

pub const ALLOWED_HIGHWAY_VALUES: [&str; 17] = [
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "residential",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
    "tertiary_link",
    "living_street",
    "track",
    "escape",
    "raceway",
    "road",
];

pub struct OsmDataReader {
    source: DataSource,
    map_data: MapDataGraph,
}

impl OsmDataReader {
    pub fn new(data_source: DataSource) -> Self {
        Self {
            map_data: MapDataGraph::new(),
            source: data_source,
        }
    }

    pub fn read_data(mut self) -> Result<MapDataGraph, OsmDataReaderError> {
        match self.source {
            DataSource::JsonFile { ref file } => {
                JsonReader::new(&mut self.map_data, file).read()?
            }
            DataSource::PbfFile { ref file } => {
                PbfReader::new(&mut self.map_data, file).read()?;
            }
        };
        Ok(self.map_data)
    }
}
