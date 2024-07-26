use std::{
    collections::{HashMap, HashSet},
    io::BufRead,
    num::{ParseFloatError, ParseIntError},
    str::Utf8Error,
    time::Instant,
};

use json_tools::{Buffer, BufferType, Lexer, Token, TokenType};

use crate::map_data_graph::{
    MapDataError, MapDataGraph, MapDataNode, MapDataWay, MapDataWayNodeIds,
};

#[derive(Debug)]
pub enum ParserError {
    UnexpectedToken {
        token: TokenType,
        context: String,
    },
    Utf8ParseError {
        error: Utf8Error,
    },
    UnexpectedBuffer,
    ArrayFoundInRoot,
    ListNotFoundInData,
    ObjectNotFoundInData,
    FailedToParseNodeId {
        error: ParseIntError,
    },
    FailedToParseLat {
        error: ParseFloatError,
    },
    FailedToParseLon {
        error: ParseFloatError,
    },
    UnknownNodeType {
        node_type: String,
    },
    UnknownMemberType {
        member_type: String,
    },
    UnknownMemberRole {
        role: String,
    },
    MissingElementType {
        element: OsmElement,
    },
    MissingValueForElement {
        element_type: OsmElementType,
        value: String,
    },
    UnableToInsertWay {
        error: MapDataError,
    },
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
    member_type: Option<OsmRelMemberType>,
    member_ref: Option<u64>,
    role: Option<OsmRelMemberRole>,
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
                self.prev_key = self.prev_string.take();
            }
            if token.kind == TokenType::Comma {
                self.prev_string = None;
            }
            if token.kind == TokenType::String || token.kind == TokenType::Number {
                if let Buffer::MultiByte(buf) = token.buf {
                    let val = std::str::from_utf8(&buf.to_owned())
                        .or_else(|error| Err(ParserError::Utf8ParseError { error }))?
                        .to_string()
                        .replace("\"", "");

                    if self.prev_key != None {
                        self.check_update_current_element(&val)?;
                        if !self.is_in_nodes_list() {
                            self.prev_key = None;
                        }
                    }
                    self.prev_string = Some(val);
                } else {
                    return Err(ParserError::UnexpectedBuffer);
                }
            }
        }

        Ok(osm_elements)
    }

    fn check_update_current_element(&mut self, val: &String) -> Result<(), ParserError> {
        if let None = self.prev_string {
            if let Some(key) = self.prev_key.clone() {
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
                } else if self.is_in_tags_obj() {
                    if let Some(ref mut current_element) = self.current_element {
                        if current_element.tags.is_none() {
                            current_element.tags = Some(HashMap::new());
                        }
                        if let Some(ref mut tags) = current_element.tags {
                            tags.insert(key, val.to_string());
                        }
                    }
                } else if self.is_in_nodes_list() {
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
                } else if self.is_in_members_obj() {
                    if let Some(current_element) = self.current_element.as_mut() {
                        if let Some(members) = current_element.members.as_mut() {
                            if let Some(member) = members.last_mut() {
                                match key.as_str() {
                                    "type" => match val.as_str() {
                                        "way" => member.member_type = Some(OsmRelMemberType::Way),
                                        "node" => member.member_type = Some(OsmRelMemberType::Node),
                                        _ => {
                                            return Err(ParserError::UnknownMemberType {
                                                member_type: val.to_string(),
                                            })
                                        }
                                    },
                                    "ref" => {
                                        let ref_id = val.parse::<u64>().or_else(|error| {
                                            Err(ParserError::FailedToParseNodeId { error })
                                        })?;
                                        member.member_ref = Some(ref_id);
                                    }
                                    "role" => match val.as_str() {
                                        "from" => member.role = Some(OsmRelMemberRole::From),
                                        "to" => member.role = Some(OsmRelMemberRole::To),
                                        "via" => member.role = Some(OsmRelMemberRole::Via),
                                        _ => {
                                            return Err(ParserError::UnknownMemberRole {
                                                role: val.to_string(),
                                            })
                                        }
                                    },
                                    _ => {}
                                }
                            }
                        }
                    }
                }
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
        } else {
            if self.is_in_members_obj() {
                if let Some(ref mut current_element) = self.current_element {
                    if current_element.members.is_none() {
                        current_element.members = Some(Vec::new());
                    }
                    if let Some(ref mut members) = current_element.members {
                        members.push(OsmRelMember {
                            member_type: None,
                            member_ref: None,
                            role: None,
                        })
                    }
                }
            }
        }
        self.prev_key = None;
        self.prev_string = None;
        Ok(())
    }

    fn set_curly_close(&mut self) -> Result<Option<OsmElement>, ParserError> {
        if let Some(loc) = self.location.last() {
            if let ParserStateLocation::InObject(loc_key) = loc {
                self.prev_key = loc_key.clone();
                self.location.pop();
            } else {
                return Err(ParserError::UnexpectedToken {
                    token: TokenType::CurlyClose,
                    context: String::from("not in a object"),
                });
            }
        }
        self.prev_string = None;

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

    fn is_in_members_list(&self) -> bool {
        if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
            if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
                if list_key == "elements" {
                    if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.get(2)
                    {
                        if obj_key == "elements" {
                            if let Some(ParserStateLocation::InList(list_key)) =
                                self.location.get(3)
                            {
                                if list_key == "members" && self.location.len() == 4 {
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
    let std_read_start = Instant::now();
    let mut map_data = MapDataGraph::new();
    let mut parser_state = OsmJsonParser::new();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line
            .expect("Could not read line from standard in")
            .as_bytes()
            .to_owned();
        let _ = parser_state.parse_line(line)?;
        // for element in elements {
        //     let element_type =
        //         element
        //             .element_type
        //             .to_owned()
        //             .ok_or(ParserError::MissingElementType {
        //                 element: element.clone(),
        //             })?;
        //     match element_type {
        //         OsmElementType::Node => map_data.insert_node(MapDataNode {
        //             id: element.id.ok_or(ParserError::MissingValueForElement {
        //                 element_type: OsmElementType::Node,
        //                 value: String::from("id"),
        //             })?,
        //             lat: element.lat.ok_or(ParserError::MissingValueForElement {
        //                 element_type: OsmElementType::Node,
        //                 value: String::from("lat"),
        //             })?,
        //             lon: element.lon.ok_or(ParserError::MissingValueForElement {
        //                 element_type: OsmElementType::Node,
        //                 value: String::from("lon"),
        //             })?,
        //         }),
        //         OsmElementType::Way => map_data
        //             .insert_way(MapDataWay {
        //                 id: element.id.ok_or(ParserError::MissingValueForElement {
        //                     element_type: OsmElementType::Way,
        //                     value: String::from("id"),
        //                 })?,
        //                 node_ids: element.nodes.map_or(
        //                     Err(ParserError::MissingValueForElement {
        //                         element_type: OsmElementType::Way,
        //                         value: String::from("node_ids"),
        //                     }),
        //                     |node_ids| Ok(MapDataWayNodeIds::from_vec(node_ids)),
        //                 )?,
        //                 one_way: element.tags.map_or(false, |tags| {
        //                     tags.get("oneway").map_or(false, |one_way| true)
        //                 }),
        //             })
        //             .map_err(|error| ParserError::UnableToInsertWay { error })?,
        //         OsmElementType::Relation => {}
        //     }
        // }
    }

    let std_read_duration = std_read_start.elapsed();
    eprintln!(
        "stdin read and serde took {} seconds",
        std_read_duration.as_secs()
    );

    Ok(map_data)
}

#[cfg(test)]
mod test {
    use crate::test_utils::get_test_data_osm_json_nodes;

    use super::OsmJsonParser;

    #[test]
    fn read_osm_json() {
        let test_data_osm_json = get_test_data_osm_json_nodes();

        let mut all_elements = Vec::new();

        let mut parser = OsmJsonParser::new();
        for line in test_data_osm_json {
            let elements = parser.parse_line(line.as_bytes().to_owned()).unwrap();
            for element in elements {
                all_elements.push(element);
            }
        }
        assert!(false);
    }
}
