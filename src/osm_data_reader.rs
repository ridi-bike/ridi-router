use std::{io::BufRead, time::Instant};

use clap::parser;
use json_tools::{Buffer, BufferType, Lexer, Token, TokenType};

use crate::map_data_graph::MapDataGraph;

enum ParserError {
    UnexpectedToken,
}

#[derive(Debug, PartialEq)]
enum ParserPrevKey {
    None,
    String(String),
    Elements,
    Nodes,
    Members,
    Tags,
}

#[derive(Debug, PartialEq)]
enum ParserStateLocation {
    InElementList,
    InElementObject,
    InNodesList,
    InMemberList,
    InMemberObject,
    InTagsObject,
    InObject,
    InList,
}

struct ParserState {
    location: Vec<ParserStateLocation>,
    prev_key: ParserPrevKey,
    prev_string: String,
}

impl ParserState {
    pub fn set_bracket_open(&mut self) -> Result<(), ParserError> {
        let last_location = self.location.last();
        if let None = last_location {
            self.location.push(ParserStateLocation::InElementList);
        } else if let Some(loc) = last_location {
            if *loc == ParserStateLocation::InElementObject {
                if self.prev_key == ParserPrevKey::Nodes {
                    self.location.push(ParserStateLocation::InNodesList);
                } else if self.prev_key == ParserPrevKey::Members {
                    self.location.push(ParserStateLocation::InMemberList);
                } else {
                    self.location.push(ParserStateLocation::InList);
                }
            }
        }
        Ok(())
    }
    pub fn set_bracket_close(&mut self) -> Result<(), ParserError> {
        if let Some(loc) = self.location.last() {
            if *loc == ParserStateLocation::InElementList
                || *loc == ParserStateLocation::InNodesList
                || *loc == ParserStateLocation::InMemberList
                || *loc == ParserStateLocation::InList
            {
                self.location.pop();
            } else {
                return Err(ParserError::UnexpectedToken);
            }
        }
        Ok(())
    }

    pub fn set_curly_open(&mut self) -> Result<(), ParserError> {
        let last_location = self.location.last();
        if let None = last_location {
            return Err(ParserError::UnexpectedToken);
        } else if let Some(loc) = last_location {
            if *loc == ParserStateLocation::InElementList {
                self.location.push(ParserStateLocation::InElementObject);
            } else if *loc == ParserStateLocation::InElementObject
                && self.prev_key == ParserPrevKey::Tags
            {
                self.location.push(ParserStateLocation::InTagsObject);
            } else if *loc == ParserStateLocation::InElementObject {
                self.location.push(ParserStateLocation::InObject);
            } else if *loc == ParserStateLocation::InMemberList {
                self.location.push(ParserStateLocation::InMemberObject);
            } else {
                self.location.push(ParserStateLocation::InObject);
            }
        }
        Ok(())
    }

    pub fn set_curly_close(&mut self) -> Result<(), ParserError> {
        if let Some(loc) = self.location.last() {
            if *loc == ParserStateLocation::InTagsObject
                || *loc == ParserStateLocation::InMemberObject
                || *loc == ParserStateLocation::InObject
                || *loc == ParserStateLocation::InElementObject
            {
                self.location.pop();
            }
        } else {
            return Err(ParserError::UnexpectedToken);
        }
        Ok(())
    }
}

pub fn read_osm_data() -> Result<MapDataGraph, ParserError> {
    let mut map_data = MapDataGraph::new();
    let std_read_start = Instant::now();
    let mut parser_state = ParserState {
        location: Vec::new(),
        prev_key: ParserPrevKey::None,
        prev_string: String::new(),
    };
    {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line
                .expect("Could not read line from standard in")
                .as_bytes()
                .to_owned();
            for token in Lexer::new(line, BufferType::Bytes(0)) {
                if token.kind == TokenType::BracketOpen {
                    parser_state.set_bracket_open();
                }
                if token.kind == TokenType::BracketClose {
                    parser_state.set_bracket_close();
                }
                if token.kind == TokenType::CurlyOpen {
                    parser_state.set_curly_open();
                }
                if token.kind == TokenType::CurlyClose {
                    parser_state.set_curly_close();
                }

                if token.kind == TokenType::String {
                    if let Buffer::MultiByte(buf) = token.buf {
                        parser_state.prev_string = std::str::from_utf8(&buf.to_owned())
                            .or_else(|e| Err(ParserError::UnexpectedToken))?
                            .to_string();
                    } else {
                        Err(ParserError::UnexpectedToken);
                    }
                }
                if token.kind == TokenType::Colon {
                    if parser_state.prev_string == "elements" {
                        parser_state.prev_key = ParserPrevKey::Elements;
                    } else if parser_state.prev_string == "tags" {
                        parser_state.prev_key = ParserPrevKey::Tags;
                    } else if parser_state.prev_string == "members" {
                        parser_state.prev_key = ParserPrevKey::Members;
                    } else if parser_state.prev_string == "nodes" {
                        parser_state.prev_key = ParserPrevKey::Nodes;
                    } else {
                        parser_state.prev_key = ParserPrevKey::String(parser_state.prev_string);
                    }
                }
            }
        }

        // let osm_data_result = serde_json::from_str::<OsmData>(&input_map_data);
        //
        // let osm_data = match osm_data_result {
        //     Ok(data) => data,
        //     Err(e) => {
        //         eprintln!("Problem parsing osm data: {e}");
        //         process::exit(1);
        //     }
        // };
        //
        // let std_read_duration = std_read_start.elapsed();
        // eprintln!(
        //     "stdin read and serde took {} seconds",
        //     std_read_duration.as_secs()
        // );
        //
        // let map_data_construct_start = Instant::now();
        //
        // for element in osm_data.elements.iter() {
        //     if element.type_field == "node" {
        //         if let (Some(lat), Some(lon)) = (element.lat, element.lon) {
        //             map_data.insert_node(MapDataNode {
        //                 id: element.id,
        //                 lat,
        //                 lon,
        //             });
        //         } else {
        //             eprintln!("Found node with missing coordinates");
        //             process::exit(1);
        //         }
        //     }
        //     if element.type_field == "way" {
        //         map_data
        //             .insert_way(MapDataWay {
        //                 id: element.id,
        //                 node_ids: MapDataWayNodeIds::from_vec(element.nodes.clone()),
        //                 one_way: element.tags.as_ref().map_or(false, |tags| {
        //                     tags.oneway
        //                         .as_ref()
        //                         .map_or(false, |one_way| one_way == "yes")
        //                 }),
        //             })
        //             .unwrap();
        //     }
        // }
        let map_data_construct_duration = map_data_construct_start.elapsed();
        eprintln!(
            "Map Data Construct took {} seconds",
            map_data_construct_duration.as_secs()
        );
    }

    map_data
}
