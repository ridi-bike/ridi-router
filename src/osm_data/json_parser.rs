use std::{
    collections::HashMap,
    num::{ParseFloatError, ParseIntError},
    str::Utf8Error,
};

use json_tools::{Buffer, BufferType, Lexer, TokenType};

use crate::map_data::osm::{
    OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType, OsmWay,
};

#[derive(Debug, PartialEq, Clone, thiserror::Error)]
pub enum OsmJsonParserError {
    #[error("Unexpected token {token:?} in context: {context}")]
    UnexpectedToken { token: TokenType, context: String },

    #[error("Failed to parse UTF-8: {error}")]
    Utf8ParseError { error: Utf8Error },

    #[error("Unexpected buffer type")]
    UnexpectedBuffer,

    #[error("Array found in root context")]
    ArrayFoundInRoot,

    #[error("Failed to parse node ID: {error}")]
    FailedToParseNodeId { error: ParseIntError },

    #[error("Failed to parse latitude: {error}")]
    FailedToParseLat { error: ParseFloatError },

    #[error("Failed to parse longitude: {error}")]
    FailedToParseLon { error: ParseFloatError },

    #[error("Unknown node type: {node_type}")]
    UnknownNodeType { node_type: String },

    #[error("Unknown member type: {member_type}")]
    UnknownMemberType { member_type: String },

    #[error("Missing element type for element: {element:?}")]
    MissingElementType { element: OsmElement },

    #[error("Missing value '{value}' for element type '{element_type}'")]
    MissingValueForElement { element_type: String, value: String },

    #[error("Parser in error state: {error}")]
    ParserInErrorState { error: Box<OsmJsonParserError> },

    #[error("Element is not a node")]
    ElementIsNotNode,

    #[error("Element is not a way")]
    ElementIsNotWay,
}

#[derive(Debug, PartialEq, Clone)]
pub enum OsmElementType {
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
    Other(String),
}

#[derive(Debug, PartialEq, Clone)]
struct OsmRelMember {
    member_type: Option<OsmRelMemberType>,
    member_ref: Option<u64>,
    role: Option<OsmRelMemberRole>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct OsmElement {
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
    pub fn get_element_type(&self) -> Result<OsmElementType, OsmJsonParserError> {
        self.element_type
            .to_owned()
            .ok_or(OsmJsonParserError::MissingElementType {
                element: self.clone(),
            })
    }

    pub fn get_node_element(&self) -> Result<OsmNode, OsmJsonParserError> {
        if let Ok(OsmElementType::Node) = self.get_element_type() {
            return Ok(OsmNode {
                id: self.id.ok_or(OsmJsonParserError::MissingValueForElement {
                    element_type: String::from("node"),
                    value: String::from("id"),
                })?,
                lat: self.lat.ok_or(OsmJsonParserError::MissingValueForElement {
                    element_type: String::from("node"),
                    value: String::from("lat"),
                })?,
                lon: self.lon.ok_or(OsmJsonParserError::MissingValueForElement {
                    element_type: String::from("node"),
                    value: String::from("lon"),
                })?,
                residential_in_proximity: false,
            });
        }

        Err(OsmJsonParserError::ElementIsNotNode)
    }

    pub fn get_way_element(&self) -> Result<OsmWay, OsmJsonParserError> {
        if let Ok(OsmElementType::Way) = self.get_element_type() {
            return Ok(OsmWay {
                id: self.id.ok_or(OsmJsonParserError::MissingValueForElement {
                    element_type: String::from("way"),
                    value: String::from("id"),
                })?,
                point_ids: self.nodes.clone().ok_or(
                    OsmJsonParserError::MissingValueForElement {
                        element_type: String::from("way"),
                        value: String::from("node_ids"),
                    },
                )?,
                tags: self.tags.clone(),
            });
        }

        Err(OsmJsonParserError::ElementIsNotWay)
    }

    pub fn get_relation_element(&self) -> Result<OsmRelation, OsmJsonParserError> {
        if let Ok(OsmElementType::Relation) = self.get_element_type() {
            return Ok(OsmRelation {
                id: self.id.ok_or(OsmJsonParserError::MissingValueForElement {
                    element_type: String::from("relation"),
                    value: String::from("id"),
                })?,
                tags: self
                    .tags
                    .clone()
                    .ok_or(OsmJsonParserError::MissingValueForElement {
                        element_type: String::from("relation"),
                        value: String::from("tags"),
                    })?,
                members: self
                    .members
                    .clone()
                    .ok_or(OsmJsonParserError::MissingValueForElement {
                        element_type: String::from("relation"),
                        value: String::from("members"),
                    })?
                    .iter()
                    .map(|member| {
                        Ok(OsmRelationMember {
                            member_type: match member.member_type.clone().ok_or(
                                OsmJsonParserError::MissingValueForElement {
                                    element_type: String::from("member"),
                                    value: String::from("member_type"),
                                },
                            )? {
                                OsmRelMemberType::Way => OsmRelationMemberType::Way,
                                OsmRelMemberType::Node => OsmRelationMemberType::Node,
                            },
                            role: match member.role.clone().ok_or(
                                OsmJsonParserError::MissingValueForElement {
                                    element_type: String::from("member"),
                                    value: String::from("role"),
                                },
                            )? {
                                OsmRelMemberRole::To => OsmRelationMemberRole::To,
                                OsmRelMemberRole::From => OsmRelationMemberRole::From,
                                OsmRelMemberRole::Via => OsmRelationMemberRole::Via,
                                OsmRelMemberRole::Other(other) => {
                                    OsmRelationMemberRole::Other(other)
                                }
                            },
                            member_ref: member.member_ref.ok_or(
                                OsmJsonParserError::MissingValueForElement {
                                    element_type: String::from("member"),
                                    value: String::from("member_role"),
                                },
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            });
        }

        Err(OsmJsonParserError::ElementIsNotWay)
    }
}

#[derive(Debug, PartialEq)]
enum ParserStateLocation {
    InObject(Option<String>),
    InList(String),
}

#[derive(Debug)]
pub struct OsmJsonParser {
    location: Vec<ParserStateLocation>,
    prev_key: Option<String>,
    prev_string: Option<String>,
    current_element: Option<OsmElement>,
    prev_error: Option<OsmJsonParserError>,
}

impl OsmJsonParser {
    pub fn new() -> Self {
        Self {
            location: Vec::new(),
            prev_key: None,
            prev_string: None,
            current_element: None,
            prev_error: None,
        }
    }

    pub fn parse_line(&mut self, line: Vec<u8>) -> Result<Vec<OsmElement>, OsmJsonParserError> {
        let parse_result = self.parse_line_internal(line);
        if let Err(error) = parse_result {
            match error {
                OsmJsonParserError::ParserInErrorState { error: _ } => {}
                _ => self.prev_error = Some(error.clone()),
            };
            return Err(error);
        }
        parse_result
    }
    fn parse_line_internal(
        &mut self,
        line: Vec<u8>,
    ) -> Result<Vec<OsmElement>, OsmJsonParserError> {
        if let Some(error) = &self.prev_error {
            return Err(OsmJsonParserError::ParserInErrorState {
                error: Box::new(error.clone()),
            });
        }
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
                        .map_err(|error| OsmJsonParserError::Utf8ParseError { error })?
                        .to_string()
                        .replace("\"", "");

                    if self.prev_key.is_some() {
                        self.check_update_current_element(&val)?;
                        if !self.is_in_nodes_list() {
                            self.prev_key = None;
                        }
                    }
                    self.prev_string = Some(val);
                } else {
                    return Err(OsmJsonParserError::UnexpectedBuffer);
                }
            }
        }

        Ok(osm_elements)
    }

    fn check_update_current_element(&mut self, val: &String) -> Result<(), OsmJsonParserError> {
        if self.prev_string.is_none() {
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
                                    return Err(OsmJsonParserError::UnknownNodeType {
                                        node_type: val.clone(),
                                    });
                                }
                            },
                            "id" => {
                                let node_id = val.parse::<u64>().map_err(|error| {
                                    OsmJsonParserError::FailedToParseNodeId { error }
                                })?;
                                current_element.id = Some(node_id)
                            }
                            "lat" => {
                                let lat = val.parse::<f64>().map_err(|error| {
                                    OsmJsonParserError::FailedToParseLat { error }
                                })?;
                                current_element.lat = Some(lat)
                            }
                            "lon" => {
                                let lon = val.parse::<f64>().map_err(|error| {
                                    OsmJsonParserError::FailedToParseLon { error }
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
                            let node_id = val.parse::<u64>().map_err(|error| {
                                OsmJsonParserError::FailedToParseNodeId { error }
                            })?;
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
                                            return Err(OsmJsonParserError::UnknownMemberType {
                                                member_type: val.to_string(),
                                            })
                                        }
                                    },
                                    "ref" => {
                                        let ref_id = val.parse::<u64>().map_err(|error| {
                                            OsmJsonParserError::FailedToParseNodeId { error }
                                        })?;
                                        member.member_ref = Some(ref_id);
                                    }
                                    "role" => match val.as_str() {
                                        "from" => member.role = Some(OsmRelMemberRole::From),
                                        "to" => member.role = Some(OsmRelMemberRole::To),
                                        "via" => member.role = Some(OsmRelMemberRole::Via),
                                        role => {
                                            member.role =
                                                Some(OsmRelMemberRole::Other(role.to_string()))
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

    fn set_bracket_open(&mut self) -> Result<(), OsmJsonParserError> {
        if let Some(key) = &self.prev_key {
            self.location
                .push(ParserStateLocation::InList(key.to_string()));
            return Ok(());
        }

        Err(OsmJsonParserError::ArrayFoundInRoot)
    }

    fn set_bracket_close(&mut self) -> Result<(), OsmJsonParserError> {
        if let Some(loc) = self.location.last() {
            if let ParserStateLocation::InList(_) = *loc {
                self.location.pop();
            } else {
                return Err(OsmJsonParserError::UnexpectedToken {
                    token: TokenType::BracketClose,
                    context: String::from("not in a list"),
                });
            }
        }
        Ok(())
    }

    fn set_curly_open(&mut self) -> Result<(), OsmJsonParserError> {
        self.location
            .push(ParserStateLocation::InObject(self.prev_key.clone()));
        if self.current_element.is_none() {
            if self.is_in_elements_obj() {
                self.current_element = Some(OsmElement::new());
            }
        } else if self.is_in_members_obj() {
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
        self.prev_key = None;
        self.prev_string = None;
        Ok(())
    }

    fn set_curly_close(&mut self) -> Result<Option<OsmElement>, OsmJsonParserError> {
        if let Some(loc) = self.location.last() {
            if let ParserStateLocation::InObject(loc_key) = loc {
                self.prev_key = loc_key.clone();
                self.location.pop();
            } else {
                return Err(OsmJsonParserError::UnexpectedToken {
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

    // fn is_in_members_list(&self) -> bool {
    //     if let Some(ParserStateLocation::InObject(None)) = self.location.first() {
    //         if let Some(ParserStateLocation::InList(list_key)) = self.location.get(1) {
    //             if list_key == "elements" {
    //                 if let Some(ParserStateLocation::InObject(Some(obj_key))) = self.location.get(2)
    //                 {
    //                     if obj_key == "elements" {
    //                         if let Some(ParserStateLocation::InList(list_key)) =
    //                             self.location.get(3)
    //                         {
    //                             if list_key == "members" && self.location.len() == 4 {
    //                                 return true;
    //                             }
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //     false
    // }
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::{osm_data::json_parser::OsmJsonParserError, test_utils::get_test_data_osm_json};

    use super::{OsmElement, OsmJsonParser, OsmRelMember, OsmRelMemberRole, OsmRelMemberType};
    pub fn get_osm_element_node(
        id: u64,
        lat: f64,
        lon: f64,
        tags: Option<Vec<(&str, &str)>>,
    ) -> OsmElement {
        OsmElement {
            element_type: Some(super::OsmElementType::Node),
            id: Some(id),
            lat: Some(lat),
            lon: Some(lon),
            tags: tags.map(|tags_vec| {
                tags_vec.iter().fold(HashMap::new(), |map, (key, val)| {
                    let mut map = map;
                    map.insert(key.to_string(), val.to_string());
                    map
                })
            }),
            nodes: None,
            members: None,
        }
    }
    pub fn get_osm_element_way(
        id: u64,
        nodes: Vec<u64>,
        tags: Option<Vec<(&str, &str)>>,
    ) -> OsmElement {
        OsmElement {
            element_type: Some(super::OsmElementType::Way),
            id: Some(id),
            lat: None,
            lon: None,
            nodes: Some(nodes),
            tags: tags.map(|tags_vec| {
                tags_vec.iter().fold(HashMap::new(), |map, (key, val)| {
                    let mut map = map;
                    map.insert(key.to_string(), val.to_string());
                    map
                })
            }),
            members: None,
        }
    }
    pub fn get_osm_element_rel(
        id: u64,
        members: Vec<(OsmRelMemberType, OsmRelMemberRole, u64)>,
        tags: Option<Vec<(&str, &str)>>,
    ) -> OsmElement {
        OsmElement {
            element_type: Some(super::OsmElementType::Relation),
            id: Some(id),
            members: Some(
                members
                    .iter()
                    .map(|(t, r, i)| OsmRelMember {
                        member_type: Some(t.clone()),
                        role: Some(r.clone()),
                        member_ref: Some(*i),
                    })
                    .collect(),
            ),
            lat: None,
            lon: None,
            tags: tags.map(|tags_vec| {
                tags_vec.iter().fold(HashMap::new(), |map, (key, val)| {
                    let mut map = map;
                    map.insert(key.to_string(), val.to_string());
                    map
                })
            }),
            nodes: None,
        }
    }

    #[test]
    fn read_osm_json() {
        let test_data_osm_json = get_test_data_osm_json();

        let mut all_elements = Vec::new();

        let mut parser = OsmJsonParser::new();
        for line in test_data_osm_json {
            let elements = parser.parse_line(line.as_bytes().to_owned()).unwrap();
            for element in elements {
                all_elements.push(element);
            }
        }

        assert_eq!(all_elements.len(), 7);

        let el = get_osm_element_node(18483373, 57.1995635, 25.0419124, None);
        assert_eq!(all_elements.first(), Some(&el));

        let el = get_osm_element_node(
            18483475,
            57.1455443,
            24.8581908,
            Some(vec![("highway", "traffic_signals")]),
        );
        assert_eq!(all_elements.get(1), Some(&el));

        let el = get_osm_element_node(18483521, 57.1485002, 24.8561211, None);
        assert_eq!(all_elements.get(2), Some(&el));

        let el = get_osm_element_way(
            80944232,
            vec![1242609397, 923273378, 923273458],
            Some(vec![
                ("highway", "living_street"),
                ("name", "Alūksnes iela"),
            ]),
        );
        assert_eq!(all_elements.get(3), Some(&el));

        let el = get_osm_element_way(
            83402701,
            vec![249790708, 1862710503],
            Some(vec![("highway", "unclassified")]),
        );
        assert_eq!(all_elements.get(4), Some(&el));

        let el = get_osm_element_rel(
            14385700,
            vec![
                (OsmRelMemberType::Way, OsmRelMemberRole::From, 37854864),
                (OsmRelMemberType::Node, OsmRelMemberRole::Via, 6721285159),
                (OsmRelMemberType::Way, OsmRelMemberRole::To, 37854864),
            ],
            Some(vec![("restriction", "no_u_turn"), ("type", "restriction")]),
        );
        assert_eq!(all_elements.get(5), Some(&el));

        let el = get_osm_element_rel(
            16896043,
            vec![
                (OsmRelMemberType::Way, OsmRelMemberRole::From, 979880972),
                (OsmRelMemberType::Node, OsmRelMemberRole::Via, 32705747),
                (OsmRelMemberType::Way, OsmRelMemberRole::To, 69666743),
            ],
            Some(vec![
                ("restriction", "no_right_turn"),
                ("type", "restriction"),
            ]),
        );
        assert_eq!(all_elements.get(6), Some(&el));
    }

    #[test]
    fn ignore_other_keys() {
        let input = vec![
            r#"{"#,
            r#"  "version": 0.6,"#,
            r#"  "generator": "Overpass API 0.7.62.1 084b4234","#,
            r#"  "osm3s": {"#,
            r#"    "timestamp_osm_base": "2024-07-23T11:01:29Z","#,
            r#"    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.""#,
            r#"  },"#,
            r#"  "elements": ["#,
            r#""#,
            r#"{"#,
            r#"  "type": "node","#,
            r#"  "id": 18483373,"#,
            r#"  "lat": 57.1995635,"#,
            r#"  "lon": 25.0419124",#,
            r#"  "some": 25.0419124",#,
            r#"  "other": "tags","#,
            r#"  "tags": {"#,
            r#"    "highway": "traffic_signals""#,
            r#"  }"#,
            r#"}"#,
            r#"  ]"#,
            r#"}"#,
        ];

        let mut all_elements = Vec::new();

        let mut parser = OsmJsonParser::new();
        for line in input {
            let elements = parser.parse_line(line.as_bytes().to_owned()).unwrap();
            for element in elements {
                all_elements.push(element);
            }
        }

        assert_eq!(all_elements.len(), 1);

        let el = get_osm_element_node(
            18483373,
            57.1995635,
            25.0419124,
            Some(vec![("highway", "traffic_signals")]),
        );
        assert_eq!(all_elements.first(), Some(&el));
    }
    #[test]
    fn return_err_on_wrong_values() {
        let input = vec![
            r#"{"#,
            r#"  "version": 0.6,"#,
            r#"  "generator": "Overpass API 0.7.62.1 084b4234","#,
            r#"  "osm3s": {"#,
            r#"    "timestamp_osm_base": "2024-07-23T11:01:29Z","#,
            r#"    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.""#,
            r#"  },"#,
            r#"  "elements": ["#,
            r#""#,
            r#"{"#,
            r#"  "type": "wrong-value","#,
            r#"  "id": 18483373,"#,
            r#"  "lat": 57.1995635,"#,
            r#"  "lon": 25.0419124",#,
            r#"}"#,
            r#"  ]"#,
            r#"}"#,
        ];

        let mut parser = OsmJsonParser::new();
        for (line_idx, &line) in input.iter().enumerate() {
            let parse_result = parser.parse_line(line.as_bytes().to_owned());
            if line_idx < 10 {
                assert_eq!(parse_result, Ok(Vec::new()));
            } else if line_idx == 10 {
                assert_eq!(
                    parse_result,
                    Err(OsmJsonParserError::UnknownNodeType {
                        node_type: String::from("wrong-value")
                    })
                );
            } else if line_idx > 10 {
                assert_eq!(
                    parse_result,
                    Err(OsmJsonParserError::ParserInErrorState {
                        error: Box::new(OsmJsonParserError::UnknownNodeType {
                            node_type: String::from("wrong-value")
                        })
                    })
                );
            }
        }

        let input = vec![
            r#"{"#,
            r#"  "version": 0.6,"#,
            r#"  "generator": "Overpass API 0.7.62.1 084b4234","#,
            r#"  "osm3s": {"#,
            r#"    "timestamp_osm_base": "2024-07-23T11:01:29Z","#,
            r#"    "copyright": "The data included in this document is from www.openstreetmap.org. The data is made available under ODbL.""#,
            r#"  },"#,
            r#"  "elements": ["#,
            r#""#,
            r#"{"#,
            r#"  "type": "relation","#,
            r#"  "id": 16896043,"#,
            r#"  "members": ["#,
            r#"    {"#,
            r#"      "type": "wrong-value","#,
            r#"      "ref": 979880972,"#,
            r#"      "role": "from""#,
            r#"    },"#,
            r#"    {"#,
            r#"      "type": "node","#,
            r#"      "ref": 32705747,"#,
            r#"      "role": "via""#,
            r#"    },"#,
            r#"    {"#,
            r#"      "type": "way","#,
            r#"      "ref": 69666743,"#,
            r#"      "role": "to""#,
            r#"    }"#,
            r#"  ],"#,
            r#"  "tags": {"#,
            r#"    "restriction": "no_right_turn","#,
            r#"    "type": "restriction""#,
            r#"  }"#,
            r#"}"#,
            r#"  ]"#,
            r#"}"#,
        ];

        let mut parser = OsmJsonParser::new();
        for (line_idx, &line) in input.iter().enumerate() {
            let parse_result = parser.parse_line(line.as_bytes().to_owned());
            if line_idx < 14 {
                assert_eq!(parse_result, Ok(Vec::new()));
            } else if line_idx == 14 {
                assert_eq!(
                    parse_result,
                    Err(OsmJsonParserError::UnknownMemberType {
                        member_type: String::from("wrong-value")
                    })
                );
            } else if line_idx > 14 {
                assert_eq!(
                    parse_result,
                    Err(OsmJsonParserError::ParserInErrorState {
                        error: Box::new(OsmJsonParserError::UnknownMemberType {
                            member_type: String::from("wrong-value")
                        })
                    })
                );
            }
        }
    }
}
