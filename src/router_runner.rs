use std::{num::ParseFloatError, path::PathBuf, string::ParseError, sync::OnceLock};

use clap::Parser;

use crate::{
    gpx_writer::RoutesWriter,
    ipc_handler::{IpcHandler, IpcHandlerError, ResponseMessage},
    map_data::graph::MapDataGraph,
    osm_data_reader::DataSource,
    result_writer::DataDestination,
    router::generator::Generator,
};

use clap::Subcommand;

#[derive(Debug)]
pub enum RouterRunnerError {
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
    Ipc {
        error: IpcHandlerError,
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

        #[arg(short, long, value_name = "FILE")]
        cache: Option<PathBuf>,
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
        cache: Option<PathBuf>,

        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        #[arg(short, long, value_name = "COORDINATES")]
        start: String,

        #[arg(short, long, value_name = "COORDINATES")]
        finish: String,
    },
}

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

pub struct RouterRunner {
    mode: RouterMode,
}

impl RouterRunner {
    pub fn init() -> Self {
        let cli = Cli::parse();
        let mode = match cli.mode {
            CliMode::Server { input, cache } => RouterMode::Server {
                data_source: get_data_source(input, cache).expect("could not get data source"),
            },
            CliMode::Client {
                output,
                start,
                finish,
            } => {
                let start_finish = get_start_finish(start, finish)
                    .expect("could not get start/finish coordinates");
                RouterMode::Client {
                    start_finish,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                }
            }
            CliMode::Dual {
                input,
                cache,
                output,
                start,
                finish,
            } => {
                let start_finish = get_start_finish(start, finish)
                    .expect("could not get start/finish coordinates");
                RouterMode::Dual {
                    data_source: get_data_source(input, cache).expect("could not get data source"),
                    start_finish,
                    data_destination: get_data_destination(output)
                        .expect("could not get data destination"),
                }
            }
        };

        Self { mode }
    }

    fn generate_route(start_finish: &StartFinish) -> Result<(), RouterRunnerError> {
        let start = match MapDataGraph::get()
            .get_closest_to_coords(start_finish.start_lat, start_finish.start_lon)
        {
            Some(p) => p,
            None => panic!("no closest point found"),
        };
        let finish = match MapDataGraph::get()
            .get_closest_to_coords(start_finish.finish_lat, start_finish.finish_lon)
        {
            Some(p) => p,
            None => panic!("no closest point found"),
        };
        let route_generator = Generator::new(start.clone(), finish.clone());
        let routes = route_generator.generate_routes();
        let writer = RoutesWriter::new(
            start.clone(),
            routes,
            start_finish.start_lat,
            start_finish.start_lon,
            None,
        );
        match writer.write_gpx() {
            Ok(()) => return Ok(()),
            Err(e) => panic!("Error on write: {:#?}", e),
        }
    }

    fn run_dual(
        &self,
        data_source: &DataSource,
        start_finish: &StartFinish,
    ) -> Result<(), RouterRunnerError> {
        MapDataGraph::init(data_source);
        RouterRunner::generate_route(start_finish)
    }

    fn run_server(&self, data_source: &DataSource) -> Result<(), RouterRunnerError> {
        MapDataGraph::init(data_source);

        let ipc = IpcHandler::init().map_err(|error| RouterRunnerError::Ipc { error })?;
        ipc.listen(|request_message| {
            RouterRunner::generate_route(&StartFinish {
                start_lat: request_message.start.lat,
                start_lon: request_message.start.lon,
                finish_lat: request_message.finish.lat,
                finish_lon: request_message.finish.lon,
            });

            ResponseMessage {
                id: request_message.id,
            }
        })
        .map_err(|error| RouterRunnerError::Ipc { error })?;
        Ok(())
    }

    fn run_client(&self, start_finish: &StartFinish) -> Result<(), RouterRunnerError> {
        let ipc = IpcHandler::init().map_err(|error| RouterRunnerError::Ipc { error })?;
        ipc.connect(start_finish)
            .map_err(|error| RouterRunnerError::Ipc { error })?;
        Ok(())
    }

    pub fn run(&self) -> Result<(), RouterRunnerError> {
        match &self.mode {
            RouterMode::Dual {
                start_finish,
                data_source,
                ..
            } => self.run_dual(&data_source, &start_finish),
            RouterMode::Server { data_source } => self.run_server(&data_source),
            RouterMode::Client { start_finish, .. } => self.run_client(&start_finish),
        }
    }
}

fn get_start_finish(start: String, finish: String) -> Result<StartFinish, RouterRunnerError> {
    let mut start = start.split(",");
    let mut finish = finish.split(",");
    Ok(StartFinish {
        start_lat: start
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Start LAT".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Start LAT".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        start_lon: start
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Start LON".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Start Lon".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        finish_lat: finish
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Finish LAT".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Finish LAT".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
        finish_lon: finish
            .next()
            .ok_or_else(|| RouterRunnerError::Coords {
                name: "Finish LON".to_string(),
                cause: "missing".to_string(),
                error: None,
            })?
            .parse()
            .map_err(|error| RouterRunnerError::Coords {
                name: "Finish LON".to_string(),
                cause: "not parsable as f64".to_string(),
                error: Some(error),
            })?,
    })
}
fn get_data_source(
    input: PathBuf,
    cache: Option<PathBuf>,
) -> Result<DataSource, RouterRunnerError> {
    if let Some(ext) = input.extension() {
        if ext == "json" {
            return Ok(DataSource::JsonFile { file: input, cache });
        } else if ext == "pbf" {
            return Ok(DataSource::PbfFile { file: input, cache });
        }
    }
    Err(RouterRunnerError::InputFileFormatIncorrect { filename: input })
}
fn get_data_destination(output: PathBuf) -> Result<DataDestination, RouterRunnerError> {
    if let Some(ext) = output.extension() {
        if ext == "json" {
            return Ok(DataDestination::Json { file: output });
        } else if ext == "gpx" {
            return Ok(DataDestination::Gpx { file: output });
        }
    }
    Err(RouterRunnerError::OutputFileFormatIncorrect { filename: output })
}
