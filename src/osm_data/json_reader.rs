use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    time::Instant,
};

use tracing::{error, trace};

use crate::{map_data::graph::MapDataGraph, osm_data::json_parser::OsmJsonParser};

use super::{
    json_parser::{OsmElement, OsmElementType},
    OsmDataReaderError,
};

pub struct JsonReader<'a> {
    map_data: &'a mut MapDataGraph,
    file_name: &'a PathBuf,
}

impl<'a> JsonReader<'a> {
    pub fn new(map_data: &'a mut MapDataGraph, file_name: &'a PathBuf) -> Self {
        Self {
            map_data,
            file_name,
        }
    }
    pub fn read(mut self) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();
        let mut parser_state = OsmJsonParser::new();

        let f =
            File::open(self.file_name).map_err(|error| OsmDataReaderError::FileError { error })?;
        let mut reader = BufReader::new(f);
        loop {
            let mut line = String::new();
            let len = reader
                .read_line(&mut line)
                .map_err(|error| OsmDataReaderError::FileError { error })?;
            if len == 0 {
                break;
            }
            let line = line.as_bytes().to_owned();
            let elements = parser_state
                .parse_line(line)
                .map_err(|error| OsmDataReaderError::ParserError { error })?;
            self.process_elements(elements)?;
        }

        self.map_data.generate_point_hashes();

        let read_duration = read_start.elapsed();
        trace!(
            read_duration_secs = read_duration.as_secs(),
            "File read done"
        );

        Ok(())
    }
    fn process_elements(&mut self, elements: Vec<OsmElement>) -> Result<(), OsmDataReaderError> {
        for element in elements {
            match element
                .get_element_type()
                .map_err(|error| OsmDataReaderError::ParserError { error })?
            {
                OsmElementType::Node => {
                    let node = element
                        .get_node_element()
                        .map_err(|error| OsmDataReaderError::ParserError { error })?;
                    self.map_data.insert_node(node);
                }
                OsmElementType::Way => {
                    let way = element
                        .get_way_element()
                        .map_err(|error| OsmDataReaderError::ParserError { error })?;
                    let res = self
                        .map_data
                        .insert_way(way)
                        .map_err(|error| OsmDataReaderError::MapDataError { error });
                    if let Err(error) = res {
                        error!(error=?error, "Error, skipping way");
                    }
                }
                OsmElementType::Relation => {
                    let rel = element
                        .get_relation_element()
                        .map_err(|error| OsmDataReaderError::ParserError { error })?;
                    let res = self
                        .map_data
                        .insert_relation(rel)
                        .map_err(|error| OsmDataReaderError::MapDataError { error });
                    if let Err(error) = res {
                        error!(error=?error, "Error, skipping relation");
                    }
                }
            }
        }
        Ok(())
    }
}
