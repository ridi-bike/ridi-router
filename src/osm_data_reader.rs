use crate::{
    map_data_graph::{MapDataError, MapDataGraph},
    osm_json_parser::{OsmElement, OsmElementType, OsmJsonParser, OsmJsonParserError},
};
use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    time::Instant,
};

#[derive(Debug)]
pub enum OsmDataReaderError {
    StdioError { error: io::Error },
    ParserError { error: OsmJsonParserError },
    MapDataError { error: MapDataError },
    FileError { error: io::Error },
}

#[derive(Debug, PartialEq)]
enum ReaderSource {
    Stdin,
    File { file: String },
}

pub struct OsmDataReader {
    source: ReaderSource,
    map_data: MapDataGraph,
}

impl OsmDataReader {
    pub fn new_stdin() -> Self {
        Self {
            source: ReaderSource::Stdin,
            map_data: MapDataGraph::new(),
        }
    }
    pub fn new_file(file: String) -> Self {
        Self {
            source: ReaderSource::File { file },
            map_data: MapDataGraph::new(),
        }
    }

    pub fn read_data(mut self) -> Result<MapDataGraph, OsmDataReaderError> {
        if self.source == ReaderSource::Stdin {
            self.read_stdin()?;
        } else if let ReaderSource::File { ref file } = self.source {
            self.read_file(file.clone())?;
        }
        Ok(self.map_data)
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
                        eprint!("Error, skipping way {:#?}", error);
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
                        eprint!("Error, skipping relation {:#?}", error);
                    }
                }
            }
        }
        Ok(())
    }

    fn read_file(&mut self, file: String) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();
        let mut parser_state = OsmJsonParser::new();

        let f = File::open(file).map_err(|error| OsmDataReaderError::FileError { error })?;
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

        let read_duration = read_start.elapsed();
        eprintln!("file read took {} seconds", read_duration.as_secs());

        Ok(())
    }

    fn read_stdin(&mut self) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();
        let mut parser_state = OsmJsonParser::new();
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line
                .map_err(|error| OsmDataReaderError::StdioError { error })?
                .as_bytes()
                .to_owned();
            let elements = parser_state
                .parse_line(line)
                .map_err(|error| OsmDataReaderError::ParserError { error })?;
            self.process_elements(elements)?;
        }

        let read_duration = read_start.elapsed();
        eprintln!("stdin read took {} seconds", read_duration.as_secs());

        Ok(())
    }
}
