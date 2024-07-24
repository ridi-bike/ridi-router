use std::{
    collections::{HashMap, HashSet},
    io::BufRead,
    num::{ParseFloatError, ParseIntError},
    str::Utf8Error,
    time::Instant,
};

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
    FailedToParseNodeId { error: ParseIntError },
    FailedToParseLat { error: ParseFloatError },
    FailedToParseLon { error: ParseFloatError },
    UnknownNodeType { node_type: String },
}

#[derive(Debug, PartialEq, Clone)]
enum OsmElementType {
    Node,
    Way,
    Relation,
}

#[derive(Debug, PartialEq, Clone)]
enum OsmRelMemberType {
    Way,
    Node,
}

#[derive(Debug, PartialEq, Clone)]
enum OsmRelMemberRole {
    From,
    Via,
    To,
}

#[derive(Debug, PartialEq, Clone)]
struct OsmRelMember {
    member_type: OsmRelMemberType,
    member_ref: u64,
    role: OsmRelMemberRole,
}

#[derive(Debug, PartialEq, Clone)]
struct OsmElement {
    element_type: Option<OsmElementType>,
    id: Option<u64>,
    tags: Option<HashMap<String, String>>,
    members: Option<Vec<OsmRelMember>>,
    nodes: Option<Vec<u64>>,
    lat: Option<f64>,
    lon: Option<f64>,
}

impl OsmElement {
    pub fn new() -> Self {
        Self {
            id: None,
            element_type: None,
            nodes: None,
            members: None,
            tags: None,
            lat: None,
            lon: None,
        }
    }
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
    current_element: Option<OsmElement>,
}

impl OsmJsonParser {
    pub fn new() -> Self {
        Self {
            location: Vec::new(),
            prev_key: None,
            prev_string: None,
            current_element: None,
        }
    }

    pub fn parse_line(&mut self, line: Vec<u8>) -> Result<Vec<OsmElement>, ParserError> {
        eprintln!("parse line {:#?}", std::str::from_utf8(&line));
        let mut osm_elements = Vec::new();
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
                let element = self.set_curly_close()?;
                if let Some(element) = element {
                    osm_elements.push(element);
                }
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
                    eprintln!("check_update_current {:#?} {:#?}", &self.prev_key, &val);
                    self.check_update_current_element(&val)?;
                    self.prev_string = Some(val);
                } else {
                    return Err(ParserError::UnexpectedBuffer);
                }
            }
        }

        eprintln!("parser {:#?}", self);
        Ok(osm_elements)
    }

    fn check_update_current_element(&mut self, val: &String) -> Result<(), ParserError> {
        if let None = self.prev_string {
            if let Some(key) = self.prev_key.clone() {
                eprintln!("{}:{}", key, val);
                if self.is_in_elements_obj() {
                    if let Some(ref mut current_element) = self.current_element {
                        match key.as_str() {
                            "type" => match val.as_str() {
                                "node" => current_element.element_type = Some(OsmElementType::Node),
                                "way" => current_element.element_type = Some(OsmElementType::Way),
                                "relation" => {
                                    current_element.element_type = Some(OsmElementType::Relation)
                                }
                                _ => {
                                    return Err(ParserError::UnknownNodeType {
                                        node_type: val.clone(),
                                    })
                                }
                            },
                            "id" => {
                                let node_id = val.parse::<u64>().or_else(|error| {
                                    Err(ParserError::FailedToParseNodeId { error })
                                })?;
                                current_element.id = Some(node_id)
                            }
                            "lat" => {
                                let lat = val.parse::<f64>().or_else(|error| {
                                    Err(ParserError::FailedToParseLat { error })
                                })?;
                                current_element.lat = Some(lat)
                            }
                            "lon" => {
                                let lon = val.parse::<f64>().or_else(|error| {
                                    Err(ParserError::FailedToParseLon { error })
                                })?;

                                current_element.lon = Some(lon)
                            }
                            _ => {}
                        }
                    }
                }
                if self.is_in_tags_obj() {
                    if let Some(ref mut current_element) = self.current_element {
                        if current_element.tags.is_none() {
                            current_element.tags = Some(HashMap::new());
                        }
                        if let Some(ref mut tags) = current_element.tags {
                            tags.insert(key, val.to_string());
                        }
                    }
                }
                if self.is_in_nodes_list() {
                    if let Some(ref mut current_element) = self.current_element {
                        if current_element.nodes.is_none() {
                            current_element.nodes = Some(Vec::new());
                        }
                        if let Some(ref mut nodes) = current_element.nodes {
                            let node_id = val
                                .parse::<u64>()
                                .or_else(|error| Err(ParserError::FailedToParseNodeId { error }))?;
                            nodes.push(node_id);
                        }
                    }
                }
                if self.is_in_members_obj() {}
            }
        }

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
        if self.current_element.is_none() {
            if self.is_in_elements_obj() {
                self.current_element = Some(OsmElement::new());
            }
        }
        Ok(())
    }

    fn set_curly_close(&mut self) -> Result<Option<OsmElement>, ParserError> {
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

        if self.is_in_elements_list() {
            return Ok(self.current_element.take());
        }

        Ok(None)
    }

    fn is_in_elements_list(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(key)) = self.location.get(1) {
                if key == "elements" && self.location.len() == 2 {
                    return true;
                }
            }
        }

        false
    }

    fn is_in_elements_obj(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
                if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.last() {
                    if list_key == "elements" && obj_key == "elements" && self.location.len() == 3 {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn is_in_members_obj(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
                if list_key == "elements" {
                    if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.get(2)
                    {
                        if obj_key == "elements" {
                            if let Some(ParserStateLocation::InList(list_key)) =
                                self.location.get(3)
                            {
                                if let Some(ParserStateLocation::InObject(Some(obj_key))) =
                                    self.location.last()
                                {
                                    if list_key == "members"
                                        && obj_key == "members"
                                        && self.location.len() == 5
                                    {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn is_in_tags_obj(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
                if list_key == "elements" {
                    if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.get(2)
                    {
                        if obj_key == "elements" {
                            if let Some(ParserStateLocation::InObject(Some(obj_key))) =
                                self.location.last()
                            {
                                if obj_key == "tags" && self.location.len() == 4 {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn is_in_nodes_list(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
                if list_key == "elements" {
                    if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.get(2)
                    {
                        if obj_key == "elements" {
                            if let Some(ParserStateLocation::InList(list_key)) =
                                self.location.last()
                            {
                                if list_key == "nodes" && self.location.len() == 4 {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
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
    use crate::test_utils::get_test_data_osm_json_nodes;

    use super::OsmJsonParser;

    #[test]
    fn read_osm_json() {
        let test_data_osm_json = get_test_data_osm_json_nodes();

        let mut parser = OsmJsonParser::new();
        for line in test_data_osm_json {
            let elements = parser.parse_line(line.as_bytes().to_owned()).unwrap();
            eprintln!("elements {:#?}", elements);
        }
        assert!(false);
    }
}
