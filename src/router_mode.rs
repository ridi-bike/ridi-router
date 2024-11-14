use std::{num::ParseFloatError, path::PathBuf, string::ParseError, sync::OnceLock};

use clap::Parser;

use crate::{osm_data_reader::DataSource, result_writer::DataDestination};

use clap::Subcommand;

#[derive(Debug)]
enum RouterModeError {
    InputFileFormatIncorrect {
        filename: PathBuf,
    },
    OutputFileFormatIncorrect {
        filename: PathBuf,
    },
    Coords {
        name: String,
        cause: String,
        error: Option<ParseFloatError>,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    pub mode: CliMode,
}

#[derive(Subcommand)]
enum CliMode {
    Server {
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
    fn get_start_finish(start: String, finish: String) -> Result<StartFinish, RouterModeError> {
        let mut start = start.split(",");
        let mut finish = finish.split(",");
        Ok(StartFinish {
            start_lat: start
                .next()
                .ok_or_else(|| RouterModeError::Coords {
                    name: "Start LAT".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterModeError::Coords {
                    name: "Start LAT".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?,
            start_lon: start
                .next()
                .ok_or_else(|| RouterModeError::Coords {
                    name: "Start LON".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterModeError::Coords {
                    name: "Start Lon".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?,
            finish_lat: finish
                .next()
                .ok_or_else(|| RouterModeError::Coords {
                    name: "Finish LAT".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterModeError::Coords {
                    name: "Finish LAT".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?,
            finish_lon: finish
                .next()
                .ok_or_else(|| RouterModeError::Coords {
                    name: "Finish LON".to_string(),
                    cause: "missing".to_string(),
                    error: None,
                })?
                .parse()
                .map_err(|error| RouterModeError::Coords {
                    name: "Finish LON".to_string(),
                    cause: "not parsable as f64".to_string(),
                    error: Some(error),
                })?,
        })
    }
    fn get_data_source(input: PathBuf) -> Result<DataSource, RouterModeError> {
        if let Some(ext) = input.extension() {
            if ext == "json" {
                return Ok(DataSource::JsonFile { file: input });
            } else if ext == "pbf" {
                return Ok(DataSource::PbfFile { file: input });
            }
        }
        Err(RouterModeError::InputFileFormatIncorrect { filename: input })
    }
    fn get_data_destination(output: PathBuf) -> Result<DataDestination, RouterModeError> {
        if let Some(ext) = output.extension() {
            if ext == "json" {
                return Ok(DataDestination::Json { file: output });
            } else if ext == "gpx" {
                return Ok(DataDestination::Gpx { file: output });
            }
        }
        Err(RouterModeError::OutputFileFormatIncorrect { filename: output })
    }
    pub fn get() -> &'static RouterMode {
        ROUTER_MODE.get_or_init(|| {
            let cli = Cli::parse();
            match cli.mode {
                CliMode::Server { input } => RouterMode::Server {
                    data_source: RouterMode::get_data_source(input)
                        .expect("could not get data source"),
                },
                CliMode::Client {
                    output,
                    start,
                    finish,
                } => {
                    let start_finish = RouterMode::get_start_finish(start, finish)
                        .expect("could not get start/finish coordinates");
                    RouterMode::Client {
                        start_finish,
                        data_destination: RouterMode::get_data_destination(output)
                            .expect("could not get data destination"),
                    }
                }
                CliMode::Dual {
                    input,
                    output,
                    start,
                    finish,
                } => {
                    let start_finish = RouterMode::get_start_finish(start, finish)
                        .expect("could not get start/finish coordinates");
                    RouterMode::Dual {
                        data_source: RouterMode::get_data_source(input)
                            .expect("could not get data source"),
                        start_finish,
                        data_destination: RouterMode::get_data_destination(output)
                            .expect("could not get data destination"),
                    }
                }
            }
        })
    }
}
