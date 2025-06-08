use crate::{
    map_data::graph::MapDataGraph,
    osm_data::{data_reader::ALLOWED_HIGHWAY_VALUES, pbf_area_reader::PbfAreaReader},
};
use geo::{Distance, Haversine, Point};
use rstar::PointDistance;
use tracing::trace;

use crate::map_data::osm::{
    OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType, OsmWay,
};
use std::{path::PathBuf, time::Instant};

use super::OsmDataReaderError;

pub struct PbfReader<'a> {
    map_data: &'a mut MapDataGraph,
    file_name: &'a PathBuf,
}

impl<'a> PbfReader<'a> {
    pub fn new(map_data: &'a mut MapDataGraph, file_name: &'a PathBuf) -> Self {
        Self {
            map_data,
            file_name,
        }
    }

    pub fn read(self) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();

        let r = std::fs::File::open(self.file_name)
            .map_err(|error| OsmDataReaderError::PbfFileOpenError { error })?;
        let mut pbf = osmpbfreader::OsmPbfReader::new(r);

        let mut boundary_reader = PbfAreaReader::new(&mut pbf);
        boundary_reader.read(|obj| obj.is_way() && obj.tags().contains("landuse", "residential"));

        let elements = pbf
            .get_objs_and_deps(|obj| {
                obj.is_way()
                    && obj.tags().iter().any(|t| {
                        t.0 == "highway"
                            && (ALLOWED_HIGHWAY_VALUES.contains(&t.1.as_str())
                                || (t.1 == "path"
                                    && obj
                                        .tags()
                                        .iter()
                                        .any(|t2| t2.0 == "motorcycle" && t2.1 == "yes")))
                    })
                    && !obj.tags().contains("motor_vehicle", "destination")
            })
            .map_err(|error| OsmDataReaderError::PbfFileReadError { error })?;

        for (_id, element) in elements {
            if element.is_node() {
                let node = element.node().ok_or(OsmDataReaderError::PbfFileError {
                    error: String::from("expected node, did not get it"),
                })?;
                self.map_data.insert_node(OsmNode {
                    id: node.id.0 as u64,
                    lat: node.lat(),
                    lon: node.lon(),
                    residential_in_proximity: match boundary_reader
                        .tree
                        .nearest_neighbor(&[node.lat(), node.lon()])
                    {
                        Some(area) => area.distance_2(&[node.lon(), node.lat()]).sqrt() <= 1000.,
                        None => false,
                    },
                });
            } else if element.is_way() {
                let way = element.way().ok_or(OsmDataReaderError::PbfFileError {
                    error: String::from("expected way, did not get it"),
                })?;
                self.map_data
                    .insert_way(OsmWay {
                        id: way.id.0 as u64,
                        point_ids: way.nodes.iter().map(|v| v.0 as u64).collect(),
                        tags: Some(
                            way.tags
                                .iter()
                                .map(|v| (v.0.to_string(), v.1.to_string()))
                                .collect(),
                        ),
                    })
                    .map_err(|error| OsmDataReaderError::MapDataError { error })?;
            } else if element.is_relation() {
                let relation = element.relation().ok_or(OsmDataReaderError::PbfFileError {
                    error: String::from("expected relation, did not get it"),
                })?;
                self.map_data
                    .insert_relation(OsmRelation {
                        id: relation.id.0 as u64,
                        members: relation
                            .refs
                            .iter()
                            .map(|v| -> Result<OsmRelationMember, OsmDataReaderError> {
                                Ok(OsmRelationMember {
                                    member_ref: match v.member {
                                        osmpbfreader::OsmId::Way(id) => id.0 as u64,
                                        osmpbfreader::OsmId::Node(id) => id.0 as u64,
                                        osmpbfreader::OsmId::Relation(id) => id.0 as u64,
                                    },
                                    role: match v.role.as_str() {
                                        "from" => OsmRelationMemberRole::From,
                                        "to" => OsmRelationMemberRole::To,
                                        "via" => OsmRelationMemberRole::Via,
                                        _ => Err(OsmDataReaderError::PbfFileError {
                                            error: String::from("unknown role"),
                                        })?,
                                    },
                                    member_type: match v.member {
                                        osmpbfreader::OsmId::Way(_) => OsmRelationMemberType::Way,
                                        osmpbfreader::OsmId::Node(_) => OsmRelationMemberType::Node,
                                        _ => Err(OsmDataReaderError::PbfFileError {
                                            error: String::from("unexpected member type"),
                                        })?,
                                    },
                                })
                            })
                            .collect::<Result<Vec<OsmRelationMember>, OsmDataReaderError>>()?,
                        tags: relation
                            .tags
                            .iter()
                            .map(|v| (v.0.to_string(), v.1.to_string()))
                            .collect(),
                    })
                    .map_err(|error| OsmDataReaderError::MapDataError { error })?;
            }
        }

        self.map_data.generate_point_hashes();

        let read_duration = read_start.elapsed();
        trace!("file read took {} seconds", read_duration.as_secs());

        Ok(())
    }
}
