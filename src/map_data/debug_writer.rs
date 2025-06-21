use std::fs::File;

use postgres::{Client, NoTls};

use crate::map_data::{line::MapDataLine, proximity::GRID_CALC_PRECISION};

pub struct MapDebugWriter {
    residential_close_file: csv::Writer<File>,
    residential_not_close_file: csv::Writer<File>,
    db: Client,
}

const RESIDENTIAL_CLOSE_FILE_NAME: &str = "map-data/res_close.csv";
const RESIDENTIAL_NOT_CLOSE_FILE_NAME: &str = "map-data/res_not_close.csv";

impl MapDebugWriter {
    pub fn new() -> Self {
        let mut client = Client::connect(
            "host=localhost port=54227 user=postgres password=password dbname=db",
            NoTls,
        )
        .expect("failed to open postgres con");
        client
            .execute("drop table if exists public.residential_close", &[])
            .expect("drop failed");

        client
            .execute("drop table if exists public.residential_not_close", &[])
            .expect("drop failed");

        client
            .execute("drop table if exists public.grid", &[])
            .expect("drop failed");

        client
            .execute(
                "create table public.residential_close (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                &[],
            )
            .expect("create failed");

        client
            .execute(
                "create table public.residential_not_close (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                &[],
            )
            .expect("create failed");

        client
            .execute(
                "create table public.grid (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                &[],
            )
            .expect("create failed");

        let mut lat = -90.;
        while lat <= 90. {
            client
                .execute(
                    "insert into grid (geom) values (ST_GeomFromText($1, 4326))",
                    &[&format!("LINESTRING(-180 {lat}, 180 {lat})")],
                )
                .expect("failed to insert");
            lat += 1. / GRID_CALC_PRECISION as f32;
        }

        let mut lon = -180.;
        while lon <= 180. {
            client
                .execute(
                    "insert into grid (geom) values (ST_GeomFromText($1, 4326))",
                    &[&format!("LINESTRING({lon} -90, {lon} 90)")],
                )
                .expect("failed to insert");
            lon += 1. / GRID_CALC_PRECISION as f32;
        }

        Self {
            residential_close_file: csv::WriterBuilder::new()
                .quote_style(csv::QuoteStyle::Never)
                .from_path(RESIDENTIAL_CLOSE_FILE_NAME)
                .expect("could not construct csv"),
            residential_not_close_file: csv::WriterBuilder::new()
                .quote_style(csv::QuoteStyle::Never)
                .from_path(RESIDENTIAL_NOT_CLOSE_FILE_NAME)
                .expect("could not construct csv"),
            db: client,
        }
    }

    pub fn write_line_residential_close(&mut self, line: ((f32, f32), (f32, f32))) -> () {
        let geom = &format!(
            "LINESTRING({} {}, {} {})",
            line.0 .1, line.0 .0, line.1 .1, line.1 .0
        );
        self.residential_close_file
            .write_record(&[geom])
            .expect("could not write to csv");
    }
    pub fn write_line_residential_not_close(&mut self, line: ((f32, f32), (f32, f32))) -> () {
        let geom = &format!(
            "LINESTRING({} {}, {} {})",
            line.0 .1, line.0 .0, line.1 .1, line.1 .0
        );
        self.residential_not_close_file
            .write_record(&[geom])
            .expect("could not write to csv");
    }

    pub fn flush(&mut self) -> () {
        self.residential_close_file
            .flush()
            .expect("Could not flush to file");
        self.residential_not_close_file
            .flush()
            .expect("Could not flush to file");

        let res_close_sql = format!(
            "copy residential_close (geom) from '/{}'",
            RESIDENTIAL_CLOSE_FILE_NAME,
        );
        let res_not_close_sql = format!(
            "copy residential_not_close (geom) from '/{}'",
            RESIDENTIAL_NOT_CLOSE_FILE_NAME,
        );

        self.db
            .execute(&res_close_sql, &[])
            .expect("could not load csv");
        self.db
            .execute(&res_not_close_sql, &[])
            .expect("could not load csv");
    }
}
