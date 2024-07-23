use std::{collections::HashMap, io::BufRead, str::Utf8Error, time::Instant};

use json_tools::{Buffer, BufferType, Lexer, Token, TokenType};

use crate::map_data_graph::MapDataGraph;

#[derive(Debug, PartialEq)]
enum ParserError {
    UnexpectedToken { token: TokenType, context: String },
    Utf8ParseError { error: Utf8Error },
    UnexpectedBuffer,
    ArrayFoundInRoot,
    ListNotFoundInData,
    ObjectNotFoundInData,
}

#[derive(Debug, PartialEq)]
enum ParserStateLocation {
    InObject(Option<String>),
    InList(String),
}

#[derive(Debug)]
struct OsmJsonParser {
    location: Vec<ParserStateLocation>,
    prev_key: Option<String>,
    prev_string: Option<String>,
    data: Vec<HashMap<String, String>>,
}

impl OsmJsonParser {
    pub fn new() -> Self {
        Self {
            location: Vec::new(),
            prev_key: None,
            prev_string: None,
            data: Vec::new(),
        }
    }

    pub fn get_current_path(&self) -> String {
        self.location
            .iter()
            .map(|loc| match loc {
                ParserStateLocation::InObject(obj) => {
                    if let Some(obj) = obj {
                        obj.clone()
                    } else {
                        String::new()
                    }
                }
                ParserStateLocation::InList(list) => list.clone(),
            })
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn parse_line(&mut self, line: Vec<u8>) -> Result<(), ParserError> {
        eprintln!("parse line {:#?}", std::str::from_utf8(&line));
        for token in Lexer::new(line, BufferType::Bytes(0)) {
            if token.kind == TokenType::BracketOpen {
                self.set_bracket_open()?;
            }
            if token.kind == TokenType::BracketClose {
                self.set_bracket_close()?;
            }
            if token.kind == TokenType::CurlyOpen {
                self.set_curly_open()?;
            }
            if token.kind == TokenType::CurlyClose {
                self.set_curly_close()?;
            }

            if token.kind == TokenType::Colon {
                self.prev_key = self.prev_string.clone();
                self.prev_string = None;
            }
            if token.kind == TokenType::String || token.kind == TokenType::Number {
                if let Buffer::MultiByte(buf) = token.buf {
                    let val = std::str::from_utf8(&buf.to_owned())
                        .or_else(|error| Err(ParserError::Utf8ParseError { error }))?
                        .to_string()
                        .replace("\"", "");
                    if let None = self.prev_string {
                        if let Some(key) = self.prev_key.clone() {
                            eprintln!("{}:{}", key, val);
                        }
                    }
                    self.prev_string = Some(val);
                } else {
                    return Err(ParserError::UnexpectedBuffer);
                }
            }
        }

        eprintln!("parser {:#?}", self);
        Ok(())
    }

    fn set_bracket_open(&mut self) -> Result<(), ParserError> {
        if let Some(key) = &self.prev_key {
            self.location
                .push(ParserStateLocation::InList(key.to_string()));
            return Ok(());
        }

        Err(ParserError::ArrayFoundInRoot)
    }

    fn set_bracket_close(&mut self) -> Result<(), ParserError> {
        if let Some(loc) = self.location.last() {
            if let ParserStateLocation::InList(_) = *loc {
                self.location.pop();
            } else {
                return Err(ParserError::UnexpectedToken {
                    token: TokenType::BracketClose,
                    context: String::from("not in a list"),
                });
            }
        }
        Ok(())
    }

    fn set_curly_open(&mut self) -> Result<(), ParserError> {
        self.location
            .push(ParserStateLocation::InObject(self.prev_key.clone()));
        Ok(())
    }

    fn set_curly_close(&mut self) -> Result<(), ParserError> {
        if let Some(loc) = self.location.last() {
            if let ParserStateLocation::InObject(_) = *loc {
                self.location.pop();
            } else {
                return Err(ParserError::UnexpectedToken {
                    token: TokenType::CurlyClose,
                    context: String::from("not in a object"),
                });
            }
        }

        Ok(())
    }
}

pub fn read_osm_data() -> Result<MapDataGraph, ParserError> {
    let mut map_data = MapDataGraph::new();
    let std_read_start = Instant::now();
    let mut parser_state = OsmJsonParser::new();
    {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line
                .expect("Could not read line from standard in")
                .as_bytes()
                .to_owned();
            parser_state.parse_line(line)?;
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
        // let map_data_construct_duration = map_data_construct_start.elapsed();
        // eprintln!(
        //     "Map Data Construct took {} seconds",
        //     map_data_construct_duration.as_secs()
        // );
    }

    // map_data
    Ok(map_data)
}

#[cfg(test)]
mod test {
    use crate::test_utils::get_test_data_osm_json;

    use super::OsmJsonParser;

    #[test]
    fn read_osm_json() {
        let test_data_osm_json = get_test_data_osm_json();

        let mut parser = OsmJsonParser::new();
        for line in test_data_osm_json {
            parser.parse_line(line.as_bytes().to_owned()).unwrap();
        }
        assert!(false);
    }
}
