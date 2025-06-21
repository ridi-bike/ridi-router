use std::{fs::File, os::unix::process::CommandExt, process::Command};

use postgres::{Client, NoTls};

pub struct MapDebugWriter {
    close: bool,
    filename: String,
    file: csv::Writer<File>,
    db: Client,
}

impl MapDebugWriter {
    pub fn new(close: bool) -> Self {
        let filename = if close {
            "map-data/close.csv"
        } else {
            "map-data/not-close.csv"
        };
        let mut client = Client::connect(
            "host=localhost port=54227 user=postgres password=password dbname=db",
            NoTls,
        )
        .expect("failed to open postgres con");
        client
            .execute("drop table if exists public.res_close", &[])
            .expect("drop failed");

        client
            .execute("drop table if exists public.res_not_close", &[])
            .expect("drop failed");

        client
            .execute(
                "create table public.res_close (
                geom GEOMETRY(LINESTRING, 4326)
            )",
                &[],
            )
            .expect("create failed");

        client
            .execute(
                "create table public.res_not_close (
                geom GEOMETRY(LINESTRING, 4326)
            )",
                &[],
            )
            .expect("create failed");

        Self {
            file: csv::WriterBuilder::new()
                .quote_style(csv::QuoteStyle::Never)
                .from_path(filename)
                .expect("could not construct csv"),
            filename: filename.to_string(),
            close,
            db: client,
        }
    }

    pub fn write_line(&mut self, geom: &str) -> () {
        self.file
            .write_record(&[geom])
            .expect("could not write to csv");
    }

    pub fn flush(&mut self) -> () {
        self.file.flush().expect("Could not flush to file");

        let copy_sql = format!(
            "copy {} (geom) from '/{}'",
            if self.close {
                "public.res_close"
            } else {
                "public.res_not_close"
            },
            self.filename
        );

        self.db.execute(&copy_sql, &[]).expect("could not load csv");
    }
}
