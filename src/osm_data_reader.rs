use osmpbf::{Element, ElementReader};

use crate::{
    map_data::{
        graph::MapDataGraph,
        osm::{OsmNode, OsmRelation, OsmWay},
        way, MapDataError,
    },
    osm_json_parser::{OsmElement, OsmElementType, OsmJsonParser, OsmJsonParserError},
};
use std::{
    collections::HashMap,
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
    PbfFileOpenError { error: osmpbf::Error },
    PbfFileReadError { error: osmpbf::Error },
}

#[derive(Debug, PartialEq)]
pub enum DataSource {
    Stdin,
    JsonFile { file: String },
    PbfFile { file: String },
}

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
            DataSource::Stdin => {
                self.read_stdin()?;
            }
            DataSource::JsonFile { ref file } => {
                self.read_json(file.clone())?;
            }
            DataSource::PbfFile { ref file } => {
                self.read_pbf(file.clone())?;
            }
        };
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

    fn read_pbf(&mut self, file: String) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();
        let reader = ElementReader::from_path(file)
            .map_err(|error| OsmDataReaderError::PbfFileOpenError { error })?;

        let elements = reader
            .par_map_reduce(
                |element| {
                    match element {
                        Element::Node(node) => (
                            vec![OsmNode {
                                id: node.id() as u64,
                                lat: node.lat(),
                                lon: node.lon(),
                            }],
                            Vec::new(),
                            Vec::new(),
                        ),
                        Element::DenseNode(dense_node) => (
                            vec![OsmNode {
                                id: dense_node.id() as u64,
                                lat: dense_node.lat(),
                                lon: dense_node.lon(),
                            }],
                            Vec::new(),
                            Vec::new(),
                        ),
                        Element::Way(way) => (
                            Vec::new(),
                            vec![OsmWay {
                                id: way.id() as u64,
                                point_ids: way.raw_refs().iter().map(|v| *v as u64).collect(),
                                tags: None,
                                // tags: Some(HashMap::from(
                                //     way.tags()
                                //         .into_iter()
                                //         .map(|v| (v.0.to_string(), v.1.to_string()))
                                //         .collect::<Vec<_>>(),
                                // )),
                            }],
                            Vec::new(),
                        ),
                        Element::Relation(relation) => (
                            Vec::new(),
                            Vec::new(),
                            vec![OsmRelation {
                                id: relation.id() as u64,
                                members: Vec::new(),
                                tags: HashMap::new(),
                            }],
                        ),
                    }
                },
                || (Vec::new(), Vec::new(), Vec::new()),
                |a, b| {
                    (
                        [a.0, b.0].concat(),
                        [a.1, b.1].concat(),
                        [a.2, b.2].concat(),
                    )
                },
            )
            .map_err(|error| OsmDataReaderError::PbfFileReadError { error })?;

        let nodes = elements.0;
        let ways = elements.1;
        let relations = elements.2;

        for node in nodes {
            self.map_data.insert_node(node);
        }
        for way in ways {
            self.map_data
                .insert_way(way)
                .map_err(|error| OsmDataReaderError::MapDataError { error })?;
        }
        for relation in relations {
            self.map_data
                .insert_relation(relation)
                .map_err(|error| OsmDataReaderError::MapDataError { error })?;
        }
        let read_duration = read_start.elapsed();
        eprintln!("file read took {} seconds", read_duration.as_secs());

        Ok(())
    }

    fn read_json(&mut self, file: String) -> Result<(), OsmDataReaderError> {
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
