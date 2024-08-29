use crate::osm_data_reader::DataSource;
use std::str::FromStr;

use clap::{arg, value_parser, Command};

#[derive(Debug)]
pub struct FromTo {
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
}

// const CLI_PARAMS: OnceLock<Cli> = OnceLock::new();

#[derive(Debug)]
pub enum Cli {
    Server {
        data_source: DataSource,
    },
    Client {
        from_to: FromTo,
    },
    Single {
        data_source: DataSource,
        from_to: FromTo,
    },
}

impl Cli {
    pub fn get() -> Self {
        // let params = CLI_PARAMS.get_or_init(|| {
        let matches = Command::new("ridi-router")
            .subcommand(
                Command::new("server")
                    .arg(
                        arg!(
                            -j --data_json <PATH> "JSON File with OSM data"
                        )
                        .value_parser(value_parser!(String)),
                    )
                    .arg(
                        arg!(
                            -p --data_pbf <PATH> "PBF with OSM data"
                        )
                        .value_parser(value_parser!(String)),
                    ),
            )
            .subcommand(
                Command::new("client")
                    .arg(
                        arg!(-f - -from <COORDINATES> "From coordinates in the format of 1.2,2.34")
                            .value_parser(value_parser!(String)),
                    )
                    .arg(
                        arg!(
                            -t --to <COORDINATES> "To coordinates in the format of 1.2,2.34"
                        )
                        .value_parser(value_parser!(String)),
                    ),
            )
            .arg(
                arg!(
                    -j --data_json <PATH> "JSON File with OSM data"
                )
                .value_parser(value_parser!(String)),
            )
            .arg(
                arg!(
                    -p --data_pbf <PATH> "PBF with OSM data"
                )
                .value_parser(value_parser!(String)),
            )
            .arg(
                arg!(-f - -from <COORDINATES> "From coordinates in the format of 1.2,2.34")
                    .value_parser(value_parser!(String))
                    .required(true),
            )
            .arg(
                arg!(
                    -t --to <COORDINATES> "To coordinates in the format of 1.2,2.34"
                )
                .value_parser(value_parser!(String))
                .required(true),
            )
            .get_matches();

        let mut from = matches
            .get_one::<String>("from")
            .expect("Value not found in the 'from' arg")
            .split(",");
        let mut to = matches
            .get_one::<String>("to")
            .expect("Value not found in the 'to' arg")
            .split(",");

        let data_json = matches.get_one::<String>("data_json");
        let data_pbf = matches.get_one::<String>("data_pbf");

        let data_source = if let Some(file) = data_json {
            DataSource::JsonFile {
                file: file.to_string(),
            }
        } else if let Some(file) = data_pbf {
            DataSource::PbfFile {
                file: file.to_string(),
            }
        } else {
            DataSource::Stdin
        };

        let from_to = FromTo {
            from_lat: from
                .next()
                .expect("LAT value not found in the 'from' arg")
                .parse()
                .expect("LAT value in the 'from' arg not parsable as f64"),
            from_lon: from
                .next()
                .expect("LON value not found in the 'from' arg")
                .parse()
                .expect("LON value in the 'from' arg not parsable as f64"),
            to_lat: to
                .next()
                .expect("LAT value not found in the 'to' arg")
                .parse()
                .expect("LAT value in the 'to' arg not parsable as f64"),
            to_lon: to
                .next()
                .expect("LON value not found in the 'to' arg")
                .parse()
                .expect("LON value in the 'to' arg not parsable as f64"),
        };

        Cli::Single {
            data_source,
            from_to,
        }
        // });
        // params.clone()
    }
}
