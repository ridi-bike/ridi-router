//! `publish-tiles`: uploads a tile generation output directory as a new release.
//! See `layout` for the published object layout.

mod layout;
mod sigv4;
mod store;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{bail, ensure, Context, Result};
use rayon::prelude::*;
use ridi_router_common::{
    format::{checksum, checksum_string, RmdfHeader, CHECKSUM_LEN},
    manifest::TileManifest,
};

use layout::{build_chunks, PublishedTile, Release, TileIndex, INDEX_KEY, INDEX_VERSION};
pub use sigv4::Credentials;
pub use store::{DirStore, ObjectStore, PutOptions, S3Store};

const UPLOAD_THREADS: usize = 16;

const IMMUTABLE: PutOptions = PutOptions {
    content_type: "application/octet-stream",
    cache_control: "public, max-age=31536000, immutable",
};
const IMMUTABLE_JSON: PutOptions = PutOptions {
    content_type: "application/json",
    cache_control: "public, max-age=31536000, immutable",
};
const INDEX: PutOptions = PutOptions {
    content_type: "application/json",
    cache_control: "public, max-age=60, must-revalidate",
};

pub struct PublishRequest {
    pub input_dir: PathBuf,
    pub store: Box<dyn ObjectStore>,
    pub prefix: String,
    pub execute: bool,
    /// Drop every tile of the previous release instead of merging into it.
    pub replace: bool,
    /// Let input edge tiles replace complete tiles of the current release (see
    /// `layout::clipping_tiles`).
    pub overwrite_edges: bool,
}

pub fn publish(request: PublishRequest) -> Result<()> {
    let prefix = normalize_prefix(&request.prefix);
    let store = request.store.as_ref();
    let key = |relative: &str| format!("{prefix}{relative}");

    let manifest = read_manifest(&request.input_dir)?;
    let format_version = manifest.format_version;
    ensure!(
        format_version == RmdfHeader::VERSION,
        "{} has RMDF format {format_version}; this build publishes format {}",
        request.input_dir.display(),
        RmdfHeader::VERSION
    );
    ensure!(
        !manifest.tiles.is_empty(),
        "{} lists no tiles; refusing to publish an empty release",
        request.input_dir.display()
    );
    ensure!(
        layout::tile_size_fits_grid(manifest.tile_size_degrees),
        "tile size {} deg does not divide 1 and 10 deg cells; apps select tiles in those cells",
        manifest.tile_size_degrees
    );
    println!(
        "Verifying {} tiles in {} ...",
        manifest.tiles.len(),
        request.input_dir.display()
    );
    verify_tiles(&request.input_dir, &manifest)?;

    let mut index = match store.get(&key(INDEX_KEY))? {
        Some(data) => {
            serde_json::from_slice::<TileIndex>(&data).context("index.json is invalid")?
        }
        None => TileIndex::default(),
    };
    let previous = index
        .releases
        .iter()
        .find(|release| release.format_version == format_version)
        .cloned();

    let mut merged: BTreeMap<(u16, u16), PublishedTile> = BTreeMap::new();
    let mut source_files: BTreeSet<String> = BTreeSet::new();
    if let (Some(previous), false) = (&previous, request.replace) {
        ensure!(
            previous.tile_size_degrees == manifest.tile_size_degrees,
            "current release {} uses {} deg tiles, these are {} deg; pass --replace to publish them \
             as a new release without merging",
            previous.release_id,
            previous.tile_size_degrees,
            manifest.tile_size_degrees
        );
        println!(
            "Merging into release {} ({} tiles) ...",
            previous.release_id, previous.tile_count
        );
        for chunk in &previous.chunks {
            let data = store
                .get(&key(&chunk.manifest))?
                .with_context(|| format!("chunk manifest {} is missing", chunk.manifest))?;
            let chunk_manifest: layout::ChunkManifest = serde_json::from_slice(&data)
                .with_context(|| format!("chunk manifest {} is invalid", chunk.manifest))?;
            for tile in chunk_manifest.tiles {
                merged.insert((tile.col, tile.row), tile);
            }
        }
        source_files.extend(previous.source_files.iter().cloned());
    }
    let known_paths: BTreeSet<String> = merged.values().map(|tile| tile.path.clone()).collect();

    let kept_edges = if request.overwrite_edges {
        BTreeSet::new()
    } else {
        let input: BTreeSet<(u16, u16)> = manifest.tiles.iter().map(|t| (t.col, t.row)).collect();
        let current: BTreeSet<(u16, u16)> = merged.keys().copied().collect();
        layout::clipping_tiles(&input, &current)
    };

    let (mut added, mut changed, mut unchanged) = (0usize, 0usize, 0usize);
    for tile in &manifest.tiles {
        if kept_edges.contains(&(tile.col, tile.row)) {
            continue;
        }
        let published = PublishedTile::from_metadata(format_version, tile);
        match merged.insert((tile.col, tile.row), published.clone()) {
            None => added += 1,
            Some(old) if old.checksum == published.checksum => unchanged += 1,
            Some(_) => changed += 1,
        }
    }
    source_files.extend(manifest.source_files.iter().cloned());

    let candidates: Vec<&PublishedTile> = merged
        .values()
        .filter(|tile| !known_paths.contains(&tile.path))
        .collect();
    let checked = AtomicUsize::new(0);
    let to_upload: Vec<&PublishedTile> = upload_pool()?.install(|| {
        candidates
            .par_iter()
            .map(|tile| {
                let exists = store.exists(&key(&tile.path))?;
                report_progress(&checked, candidates.len(), "checked");
                Ok((!exists).then_some(*tile))
            })
            .collect::<Result<Vec<_>>>()
            .map(|tiles| tiles.into_iter().flatten().collect())
    })?;
    let upload_bytes: u64 = to_upload.iter().map(|tile| tile.size_bytes).sum();
    let total_bytes: u64 = merged.values().map(|tile| tile.size_bytes).sum();

    println!();
    println!("Target:     {} (prefix {prefix:?})", store.describe());
    println!(
        "Release:    format v{format_version}, {} deg tiles, {} tiles, {}",
        manifest.tile_size_degrees,
        merged.len(),
        format_bytes(total_bytes)
    );
    match (&previous, request.replace) {
        (Some(previous), true) => println!(
            "Replacing:  release {} ({} tiles) without merging",
            previous.release_id, previous.tile_count
        ),
        (Some(previous), false) => println!("Previous:   release {}", previous.release_id),
        (None, _) => println!("Previous:   none for format v{format_version}"),
    }
    println!("Input:      {added} new, {changed} changed, {unchanged} unchanged");
    if !kept_edges.is_empty() {
        println!(
            "Kept:       {} edge tiles of the current release; the input holds only its own side of \
             them (generate neighbouring regions together, or pass --overwrite-edges)",
            kept_edges.len()
        );
    }
    println!(
        "Upload:     {} tile objects, {}",
        to_upload.len(),
        format_bytes(upload_bytes)
    );

    if !request.execute {
        println!();
        println!("Dry run, nothing written. Pass --execute to publish.");
        return Ok(());
    }

    let uploaded = AtomicUsize::new(0);
    upload_pool()?.install(|| {
        to_upload.par_iter().try_for_each(|tile| -> Result<()> {
            let data = fs::read(request.input_dir.join(&tile.filename))
                .with_context(|| format!("reading {}", tile.filename))?;
            store.put(&key(&tile.path), &data, &IMMUTABLE)?;
            report_progress(&uploaded, to_upload.len(), "uploaded");
            Ok(())
        })
    })?;

    let release_id = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    let chunks = build_chunks(
        format_version,
        &release_id,
        manifest.tile_size_degrees,
        merged.into_values(),
    );
    for (summary, chunk_manifest) in &chunks {
        store.put(
            &key(&summary.manifest),
            &serde_json::to_vec(chunk_manifest)?,
            &IMMUTABLE_JSON,
        )?;
    }

    let release = Release {
        format_version,
        release_id: release_id.clone(),
        published_at: chrono::Utc::now().to_rfc3339(),
        tile_size_degrees: manifest.tile_size_degrees,
        chunk_size_degrees: layout::CHUNK_SIZE_DEGREES,
        tile_count: chunks.iter().map(|(summary, _)| summary.tile_count).sum(),
        total_bytes,
        source_files: source_files.into_iter().collect(),
        chunks: chunks.into_iter().map(|(summary, _)| summary).collect(),
    };
    index.index_version = INDEX_VERSION;
    index
        .releases
        .retain(|existing| existing.format_version != format_version);
    index.releases.push(release);
    index
        .releases
        .sort_by(|a, b| b.format_version.cmp(&a.format_version));
    store.put(&key(INDEX_KEY), &serde_json::to_vec_pretty(&index)?, &INDEX)?;

    println!("Published release {release_id} ({}).", key(INDEX_KEY));
    Ok(())
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

fn read_manifest(input_dir: &Path) -> Result<TileManifest> {
    let path = input_dir.join("manifest.json");
    let data = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("parsing {}", path.display()))
}

/// Every tile must exist, match its manifest size, and carry the checksum the manifest lists.
fn verify_tiles(input_dir: &Path, manifest: &TileManifest) -> Result<()> {
    let failures: Vec<String> = manifest
        .tiles
        .par_iter()
        .filter_map(|tile| {
            let path = input_dir.join(&tile.filename);
            let data = match fs::read(&path) {
                Ok(data) => data,
                Err(error) => return Some(format!("{}: {error}", tile.filename)),
            };
            if data.len() as u64 != tile.size_bytes || data.len() < CHECKSUM_LEN {
                return Some(format!(
                    "{}: {} bytes, manifest says {}",
                    tile.filename,
                    data.len(),
                    tile.size_bytes
                ));
            }
            let (body, trailer) = data.split_at(data.len() - CHECKSUM_LEN);
            let sum = checksum(body);
            if trailer != sum || checksum_string(&sum) != tile.checksum {
                return Some(format!("{}: checksum mismatch", tile.filename));
            }
            None
        })
        .collect();
    if !failures.is_empty() {
        let shown = failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ");
        bail!(
            "{} of {} tiles failed verification:\n  {shown}",
            failures.len(),
            manifest.tiles.len()
        );
    }
    Ok(())
}

fn upload_pool() -> Result<rayon::ThreadPool> {
    Ok(rayon::ThreadPoolBuilder::new()
        .num_threads(UPLOAD_THREADS)
        .build()?)
}

fn report_progress(counter: &AtomicUsize, total: usize, verb: &str) {
    let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if done % 500 == 0 || done == total {
        println!("  {verb} {done}/{total}");
    }
}

fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.1} MB")
    }
}
