use serde::{Deserialize, Serialize};

use crate::{
    map_data::{
        graph::{MapDataGraph, MapDataGraphPacked},
        osm::{
            OsmNode, OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType,
            OsmWay,
        },
        MapDataError,
    },
    osm_json_parser::{OsmElement, OsmElementType, OsmJsonParser, OsmJsonParserError},
};
use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    time::Instant,
};

#[derive(Debug)]
pub enum OsmDataReaderError {
    StdioError { error: io::Error },
    ParserError { error: OsmJsonParserError },
    MapDataError { error: MapDataError },
    FileError { error: io::Error },
    PbfFileOpenError { error: io::Error },
    PbfFileReadError { error: osmpbfreader::Error },
    PbfFileError { error: String },
}

#[derive(Debug, PartialEq, Clone)]
pub enum DataSource {
    JsonFile {
        file: PathBuf,
        cache: Option<PathBuf>,
    },
    PbfFile {
        file: PathBuf,
        cache: Option<PathBuf>,
    },
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
            DataSource::JsonFile {
                ref file,
                ref cache,
            } => {
                let file = file.clone();
                let cache = cache.clone();
                let cache_read = if let Some(cache) = &cache {
                    self.read_cache(cache.clone())?
                } else {
                    false
                };

                if !cache_read {
                    self.read_json(file, cache)?;
                }
            }
            DataSource::PbfFile {
                ref file,
                ref cache,
            } => {
                let file = file.clone();
                let cache = cache.clone();
                let cache_read = if let Some(cache) = &cache {
                    self.read_cache(cache.clone())?
                } else {
                    false
                };

                if !cache_read {
                    self.read_pbf(file, cache)?;
                }
            }
        };
        Ok(self.map_data)
    }

    fn read_cache(&mut self, cache_file: PathBuf) -> Result<bool, OsmDataReaderError> {
        let read_start = Instant::now();
        // let file = match std::fs::File::open(cache_file) {
        //     Err(_) => return Ok(false),
        //     Ok(c) => c,
        // };
        // let graph = bincode::deserialize_from(file).expect("could not deserialize");

        // let file_contents = match std::fs::read(cache_file) {
        //     Err(_) => return Ok(false),
        //     Ok(c) => c,
        // };
        // let graph = bincode::deserialize(&file_contents[..]).expect("could not deserialize");
        // self.map_data = graph;

        if !std::fs::exists(&cache_file).expect("could not check if cache exists") {
            return Ok(false);
        }

        let mut packed_data = MapDataGraphPacked::default();
        fn read_cache_file(file_folder: &PathBuf, file_name: &str) -> Vec<u8> {
            let mut file = file_folder.clone();
            file.push(format!("{}.cache", file_name));
            let file_contents =
                std::fs::read(file).expect(format!("could not read {file_name} file").as_str());
            // bincode::deserialize(&file_contents[..])
            //     .expect(format!("could not deserialize {file_name} file").as_str())
            file_contents
        }

        rayon::scope(|scope| {
            scope.spawn(|_| {
                packed_data.points = read_cache_file(&cache_file, "points");
            });
            scope.spawn(|_| {
                packed_data.points_hashed_offset_none =
                    read_cache_file(&cache_file, "points_hashed_offset_none");
            });
            scope.spawn(|_| {
                packed_data.points_hashed_offset_lat =
                    read_cache_file(&cache_file, "points_hashed_offset_lat");
            });
            scope.spawn(|_| {
                packed_data.points_hashed_offset_lon =
                    read_cache_file(&cache_file, "points_hashed_offset_lon");
            });
            scope.spawn(|_| {
                packed_data.points_hashed_offset_lat_lon =
                    read_cache_file(&cache_file, "points_hashed_offset_lat_lon");
            });
            scope.spawn(|_| {
                packed_data.lines = read_cache_file(&cache_file, "lines");
            });
            scope.spawn(|_| {
                packed_data.tags = read_cache_file(&cache_file, "tags");
            });
        });

        self.map_data = MapDataGraph::unpack(packed_data);

        let read_duration = read_start.elapsed();
        eprintln!("cache read took {} seconds", read_duration.as_secs());
        Ok(true)
    }

    fn write_cache(&self, cache_file: Option<PathBuf>) -> Result<(), OsmDataReaderError> {
        if let Some(cache_file) = cache_file {
            // let file = std::fs::File::create(cache_file).expect("could not create file");
            // bincode::serialize_into(file, &self.map_data).expect("could not serialize into writer");

            // let serialized_graph =
            //     bincode::serialize(&self.map_data).expect("could not serialize into writer");
            // std::fs::write(cache_file, serialized_graph).expect("failed to write to file");
            if std::fs::exists(&cache_file).expect("could not check dir") {
                std::fs::remove_dir_all(&cache_file).expect("could not delete dir");
            }
            std::fs::create_dir_all(&cache_file).expect("could not create dir");
            fn write_cache_file(
                file_folder: &PathBuf,
                file_name: &str,
                file_contents: Vec<u8>,
            ) -> () {
                let mut file = file_folder.clone();
                file.push(format!("{file_name}.cache"));
                std::fs::write(file, file_contents)
                    .expect(format!("could not write {file_name} to file").as_str());
            }
            let packed_data = self.map_data.pack();
            rayon::scope(|scope| {
                scope.spawn(|_| {
                    write_cache_file(&cache_file, "points", packed_data.points);
                });
                scope.spawn(|_| {
                    write_cache_file(
                        &cache_file,
                        "points_hashed_offset_none",
                        packed_data.points_hashed_offset_none,
                    );
                });
                scope.spawn(|_| {
                    write_cache_file(
                        &cache_file,
                        "points_hashed_offset_lat",
                        packed_data.points_hashed_offset_lat,
                    );
                });
                scope.spawn(|_| {
                    write_cache_file(
                        &cache_file,
                        "points_hashed_offset_lon",
                        packed_data.points_hashed_offset_lon,
                    );
                });
                scope.spawn(|_| {
                    write_cache_file(
                        &cache_file,
                        "points_hashed_offset_lat_lon",
                        packed_data.points_hashed_offset_lat_lon,
                    );
                });
                scope.spawn(|_| {
                    write_cache_file(&cache_file, "lines", packed_data.lines);
                });
                scope.spawn(|_| {
                    write_cache_file(&cache_file, "tags", packed_data.tags);
                });
            })
        }
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

    fn read_pbf(
        &mut self,
        file: PathBuf,
        cache_file: Option<PathBuf>,
    ) -> Result<(), OsmDataReaderError> {
        let read_start = Instant::now();

        let path = std::path::Path::new(&file);
        let r = std::fs::File::open(&path)
            .map_err(|error| OsmDataReaderError::PbfFileOpenError { error })?;
        let mut pbf = osmpbfreader::OsmPbfReader::new(r);

        let elements = pbf
            .get_objs_and_deps(|obj| {
                obj.is_way()
                    && obj.tags().iter().any(|t| {
                        t.0 == "highway"
                            && t.1 != "proposed"
                            && t.1 != "cycleway"
                            && t.1 != "steps"
                            && t.1 != "pedestrian"
                            && t.1 != "path"
                            && t.1 != "service"
                            && t.1 != "footway"
                    })
            })
            .map_err(|error| OsmDataReaderError::PbfFileReadError { error })?;

        for (_id, element) in elements {
            if element.is_node() {
                let node = element.node().map_or(
                    Err(OsmDataReaderError::PbfFileError {
                        error: String::from("expected node, did not get it"),
                    }),
                    |v| Ok(v),
                )?;
                self.map_data.insert_node(OsmNode {
                    id: node.id.0 as u64,
                    lat: node.lat(),
                    lon: node.lon(),
                });
            } else if element.is_way() {
                let way = element.way().map_or(
                    Err(OsmDataReaderError::PbfFileError {
                        error: String::from("expected way, did not get it"),
                    }),
                    |v| Ok(v),
                )?;
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
                let relation = element.relation().map_or(
                    Err(OsmDataReaderError::PbfFileError {
                        error: String::from("expected relation, did not get it"),
                    }),
                    |v| Ok(v),
                )?;
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

        self.write_cache(cache_file).expect("could not write cache");

        let read_duration = read_start.elapsed();
        eprintln!("file read took {} seconds", read_duration.as_secs());

        Ok(())
    }

    fn read_json(
        &mut self,
        file: PathBuf,
        cache_file: Option<PathBuf>,
    ) -> Result<(), OsmDataReaderError> {
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

        self.map_data.generate_point_hashes();

        self.write_cache(cache_file).expect("could not write cache");

        let read_duration = read_start.elapsed();
        eprintln!("file read took {} seconds", read_duration.as_secs());

        Ok(())
    }
}
