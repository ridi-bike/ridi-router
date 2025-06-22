use std::fs::File;
use wkt::ToWkt;

use geo::{Coord, LineString, MultiPolygon};
use postgres::{Client, NoTls};

use crate::map_data::proximity::{round_to_precision, GRID_CALC_PRECISION};

pub struct MapDebugWriter {
    residential_close_file: Option<csv::Writer<File>>,
    residential_not_close_file: Option<csv::Writer<File>>,
    residential_area_adjusted_file: Option<csv::Writer<File>>,
    residential_area_file: Option<csv::Writer<File>>,
    grid_file: Option<csv::Writer<File>>,
}

const RESIDENTIAL_CLOSE_FILE_NAME: &str = "map-data/res_close.csv";
const RESIDENTIAL_NOT_CLOSE_FILE_NAME: &str = "map-data/res_not_close.csv";
const RESIDENTIAL_AREA_ADJUSTED_FILE_NAME: &str = "map-data/residential_adjusted.csv";
const RESIDENTIAL_AREA_FILE_NAME: &str = "map-data/residential.csv";
const GRID_FILE_NAME: &str = "map-data/grid.csv";

impl MapDebugWriter {
    pub fn new() -> Self {
        Self {
            residential_close_file: None,
            residential_not_close_file: None,
            residential_area_adjusted_file: None,
            residential_area_file: None,
            grid_file: None,
        }
    }

    pub fn write_line_residential_close(&mut self, line: &LineString) -> () {
        let geom = line.to_wkt().to_string();
        if self.residential_close_file.is_none() {
            eprintln!("new new new close");
            self.residential_close_file = Some(
                csv::WriterBuilder::new()
                    .quote_style(csv::QuoteStyle::Never)
                    .from_path(RESIDENTIAL_CLOSE_FILE_NAME)
                    .expect("could not construct csv"),
            );
        }
        if let Some(ref mut file) = self.residential_close_file {
            file.write_record(&[geom]).expect("could not write to csv");
        }
    }
    pub fn write_line_residential_not_close(&mut self, line: &LineString) -> () {
        let geom = line.to_wkt().to_string();
        if self.residential_not_close_file.is_none() {
            eprintln!("new new new not close");
            self.residential_not_close_file = Some(
                csv::WriterBuilder::new()
                    .quote_style(csv::QuoteStyle::Never)
                    .from_path(RESIDENTIAL_NOT_CLOSE_FILE_NAME)
                    .expect("could not construct csv"),
            );
        }
        if let Some(ref mut file) = self.residential_not_close_file {
            file.write_record(&[geom]).expect("could not write to csv");
        }
    }
    pub fn write_area_residential_adjusted(&mut self, area: &MultiPolygon) -> () {
        let geom = area.to_wkt().to_string();
        if self.residential_area_adjusted_file.is_none() {
            self.residential_area_adjusted_file = Some(
                csv::WriterBuilder::new()
                    .quote_style(csv::QuoteStyle::Never)
                    .from_path(RESIDENTIAL_AREA_ADJUSTED_FILE_NAME)
                    .expect("could not construct csv"),
            );
        }
        if let Some(ref mut file) = self.residential_area_adjusted_file {
            file.write_record(&[geom]).expect("could not write to csv");
        }
    }
    pub fn write_area_residential(&mut self, area: &MultiPolygon) -> () {
        let geom = area.to_wkt().to_string();
        if self.residential_area_file.is_none() {
            self.residential_area_file = Some(
                csv::WriterBuilder::new()
                    .quote_style(csv::QuoteStyle::Never)
                    .from_path(RESIDENTIAL_AREA_FILE_NAME)
                    .expect("could not construct csv"),
            );
        }
        if let Some(ref mut file) = self.residential_area_file {
            file.write_record(&[geom]).expect("could not write to csv");
        }
    }
    pub fn write_line_grid(&mut self) -> () {
        if self.grid_file.is_none() {
            self.grid_file = Some(
                csv::WriterBuilder::new()
                    .quote_style(csv::QuoteStyle::Never)
                    .from_path(GRID_FILE_NAME)
                    .expect("could not construct csv"),
            );
        }
        if let Some(ref mut file) = self.grid_file {
            let mut lat = -90.;
            while lat <= 90. {
                eprintln!("lat: {lat}");
                let line =
                    LineString::new(vec![Coord { x: -180., y: lat }, Coord { x: 180., y: lat }]);

                file.write_record(&[line.to_wkt().to_string()])
                    .expect("could not write to csv");
                lat = round_to_precision(lat + 1. / GRID_CALC_PRECISION as f64);
            }

            let mut lon = -180.;
            while lon <= 180. {
                eprintln!("lon: {lon}");
                let line =
                    LineString::new(vec![Coord { x: lon, y: -90. }, Coord { x: lon, y: 90. }]);

                file.write_record(&[line.to_wkt().to_string()])
                    .expect("could not write to csv");
                lon = round_to_precision(lon + 1. / GRID_CALC_PRECISION as f64);
            }
        }
    }

    pub fn flush(&mut self) -> () {
        let mut client = Client::connect(
            "host=localhost port=54227 user=postgres password=password dbname=db",
            NoTls,
        )
        .expect("failed to open postgres con");

        if let Some(ref mut file) = self.residential_close_file {
            client
                .execute("drop table if exists public.residential_close", &[])
                .expect("drop failed");
            client
                .execute(
                    "create table public.residential_close (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                    &[],
                )
                .expect("create failed");
            file.flush().expect("Could not flush to file");
            let res_close_sql = format!(
                "copy residential_close (geom) from '/{}' with delimiter ';' csv",
                RESIDENTIAL_CLOSE_FILE_NAME,
            );
            client
                .execute(&res_close_sql, &[])
                .expect("could not load csv");
        }
        if let Some(ref mut file) = self.residential_not_close_file {
            client
                .execute("drop table if exists public.residential_not_close", &[])
                .expect("drop failed");
            client
                .execute(
                    "create table public.residential_not_close (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                    &[],
                )
                .expect("create failed");
            file.flush().expect("Could not flush to file");
            let res_not_close_sql = format!(
                "copy residential_not_close (geom) from '/{}' with delimiter ';' csv",
                RESIDENTIAL_NOT_CLOSE_FILE_NAME,
            );
            client
                .execute(&res_not_close_sql, &[])
                .expect("could not load csv");
        }
        if let Some(ref mut file) = self.residential_area_adjusted_file {
            client
                .execute("drop table if exists public.residential_adjusted", &[])
                .expect("drop failed");
            client
                .execute(
                    "create table public.residential_adjusted (
                    geom GEOMETRY(MULTIPOLYGON, 4326)
                )",
                    &[],
                )
                .expect("create failed");
            file.flush().expect("Could not flush to file");
            let res_adjusted_sql = format!(
                "copy residential_adjusted (geom) from '/{}' with delimiter ';' csv",
                RESIDENTIAL_AREA_ADJUSTED_FILE_NAME,
            );
            client
                .execute(&res_adjusted_sql, &[])
                .expect("could not load csv");
        }
        if let Some(ref mut file) = self.residential_area_file {
            client
                .execute("drop table if exists public.residential", &[])
                .expect("drop failed");
            client
                .execute(
                    "create table public.residential (
                    geom GEOMETRY(MULTIPOLYGON, 4326)
                )",
                    &[],
                )
                .expect("create failed");
            file.flush().expect("Could not flush to file");
            let res_sql = format!(
                "copy residential (geom) from '/{}' with delimiter ';' csv",
                RESIDENTIAL_AREA_FILE_NAME,
            );

            client.execute(&res_sql, &[]).expect("could not load csv");
        }
        if let Some(ref mut file) = self.grid_file {
            client
                .execute("drop table if exists public.grid", &[])
                .expect("drop failed");
            client
                .execute(
                    "create table public.grid (
                    geom GEOMETRY(LINESTRING, 4326)
                )",
                    &[],
                )
                .expect("create failed");
            file.flush().expect("Could not flush to file");
            let grid_sql = format!(
                "copy grid (geom) from '/{}' with delimiter ';' csv",
                GRID_FILE_NAME
            );

            client.execute(&grid_sql, &[]).expect("could not load csv");
        }
    }
}
