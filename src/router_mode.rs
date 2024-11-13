use core::panic;
use std::{path::PathBuf, sync::OnceLock};

use clap::Parser;

use crate::{osm_data_reader::DataSource, result_writer::DataDestination};

use clap::Subcommand;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    pub mode: CliMode,
}

#[derive(Subcommand)]
enum CliMode {
    Sever {
        #[arg(short, long, value_name = "FILE")]
        input: PathBuf,
    },
    Client {
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        #[arg(short, long, value_name = "COORDINATES")]
        start: String,

        #[arg(short, long, value_name = "COORDINATES")]
        finish: String,
    },
    Dual {
        #[arg(short, long, value_name = "FILE")]
        input: PathBuf,

        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        #[arg(short, long, value_name = "COORDINATES")]
        start: String,

        #[arg(short, long, value_name = "COORDINATES")]
        finish: String,
    },
}

pub static ROUTER_MODE: OnceLock<RouterMode> = OnceLock::new();

#[derive(Debug)]
pub struct StartFinish {
    pub start_lat: f64,
    pub start_lon: f64,
    pub finish_lat: f64,
    pub finish_lon: f64,
}

#[derive(Debug)]
pub enum RouterMode {
    Server {
        data_source: DataSource,
    },
    Client {
        start_finish: StartFinish,
        data_destination: DataDestination,
    },
    Dual {
        data_source: DataSource,
        start_finish: StartFinish,
        data_destination: DataDestination,
    },
}

impl RouterMode {
    fn get_start_finish(start: String, finish: String) -> StartFinish {
        let mut start = start.split(",");
        let mut finish = finish.split(",");
        StartFinish {
            start_lat: start
                .next()
                .expect("LAT value not found in the 'start' arg")
                .parse()
                .expect("LAT value in the 'start' arg not parsable as f64"),
            start_lon: start
                .next()
                .expect("LON value not found in the 'start' arg")
                .parse()
                .expect("LON value in the 'start' arg not parsable as f64"),
            finish_lat: finish
                .next()
                .expect("LAT value not found in the 'finish' arg")
                .parse()
                .expect("LAT value in the 'finish' arg not parsable as f64"),
            finish_lon: finish
                .next()
                .expect("LON value not found in the 'finish' arg")
                .parse()
                .expect("LON value in the 'finish' arg not parsable as f64"),
        }
    }
    fn get_data_source(input: PathBuf) -> DataSource {
        if input.ends_with(".json") {
            DataSource::JsonFile { file: input }
        } else if input.ends_with(".pbf") {
            DataSource::PbfFile { file: input }
        } else {
            panic!("expected json or pbf")
        }
    }
    fn get_data_destination(output: PathBuf) -> DataDestination {
        if output.ends_with(".json") {
            DataDestination::Json { file: output }
        } else if output.ends_with(".gpx") {
            DataDestination::Gpx { file: output }
        } else {
            panic!("output can only be json or gpx");
        }
    }
    pub fn get() -> &'static RouterMode {
        ROUTER_MODE.get_or_init(|| {
            let cli = Cli::parse();
            match cli.mode {
                CliMode::Sever { input } => RouterMode::Server {
                    data_source: RouterMode::get_data_source(input),
                },
                CliMode::Client {
                    output,
                    start,
                    finish,
                } => {
                    let start_finish = RouterMode::get_start_finish(start, finish);
                    RouterMode::Client {
                        start_finish,
                        data_destination: RouterMode::get_data_destination(output),
                    }
                }
                CliMode::Dual {
                    input,
                    output,
                    start,
                    finish,
                } => {
                    let start_finish = RouterMode::get_start_finish(start, finish);
                    RouterMode::Dual {
                        data_source: RouterMode::get_data_source(input),
                        start_finish,
                        data_destination: RouterMode::get_data_destination(output),
                    }
                }
            }
        })
    }
}
