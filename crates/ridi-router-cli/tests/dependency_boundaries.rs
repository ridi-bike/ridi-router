use std::{fs, path::{Path, PathBuf}};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn crate_root(crate_name: &str) -> PathBuf {
    workspace_root().join("crates").join(crate_name)
}

fn cargo_toml(crate_name: &str) -> String {
    fs::read_to_string(crate_root(crate_name).join("Cargo.toml")).unwrap()
}

fn read_rs_files(dir: &Path, buf: &mut String) {
    let mut entries = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            read_rs_files(&path, buf);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            buf.push_str(&fs::read_to_string(path).unwrap());
            buf.push('\n');
        }
    }
}

fn crate_source(crate_name: &str) -> String {
    let mut source = String::new();
    read_rs_files(&crate_root(crate_name).join("src"), &mut source);
    source
}

#[test]
fn routing_crate_source_contains_no_clap_dependency_usage() {
    let source = crate_source("ridi-router-routing");
    assert!(!source.contains("clap::"));
    assert!(!source.contains("use clap"));
}

#[test]
fn routing_crate_source_contains_no_json_writer_usage() {
    let source = crate_source("ridi-router-routing");
    assert!(!source.contains("json_writer"));
    assert!(!source.contains("JsonWriter"));
}

#[test]
fn routing_crate_source_contains_no_gpx_writer_usage() {
    let source = crate_source("ridi-router-routing");
    assert!(!source.contains("gpx_writer"));
    assert!(!source.contains("GpxWriter"));
}

#[test]
fn tiles_crate_source_contains_no_clap_dependency_usage() {
    let source = crate_source("ridi-router-tiles");
    assert!(!source.contains("clap::"));
    assert!(!source.contains("use clap"));
}

#[test]
fn cli_depends_on_routing_and_tiles_but_reverse_is_not_true() {
    let cli = cargo_toml("ridi-router-cli");
    let routing = cargo_toml("ridi-router-routing");
    let tiles = cargo_toml("ridi-router-tiles");

    assert!(cli.contains("ridi-router-routing"));
    assert!(cli.contains("ridi-router-tiles"));

    assert!(!routing.contains("ridi-router-cli"));
    assert!(!routing.contains("ridi-router-tiles"));

    assert!(!tiles.contains("ridi-router-cli"));
    assert!(!tiles.contains("ridi-router-routing"));
}
