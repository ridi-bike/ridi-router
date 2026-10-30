//! rmdf-diff: semantic comparison of RMDF tile directories.
//!
//! Usage:
//!   rmdf-diff <ref_dir> <candidate_dir> [--json <file>] [--max-examples N]
//!   rmdf-diff --digest <dir>
//!
//! Exit code: 0 = semantically identical (empty tiles ignored), no candidate consistency issues
//! and no candidate manifest issues (digest: no consistency issues); 1 = differences; 2 =
//! usage/IO error.

use std::path::PathBuf;

use anyhow::{Context, Result};

fn usage() -> ! {
    eprintln!(
        "usage: rmdf-diff <ref_dir> <candidate_dir> [--json <file>] [--max-examples N]\n       rmdf-diff --digest <dir>"
    );
    std::process::exit(2);
}

fn main() {
    match run() {
        Ok(ok) => std::process::exit(if ok { 0 } else { 1 }),
        Err(e) => {
            eprintln!("rmdf-diff: error: {e:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<bool> {
    let mut pos = Vec::new();
    let mut json: Option<PathBuf> = None;
    let mut digest: Option<PathBuf> = None;
    let mut max_ex = 10usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => json = Some(args.next().unwrap_or_else(|| usage()).into()),
            "--digest" => digest = Some(args.next().unwrap_or_else(|| usage()).into()),
            "--max-examples" => {
                max_ex = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "-h" | "--help" => usage(),
            _ => pos.push(PathBuf::from(a)),
        }
    }
    if let Some(dir) = digest {
        if !pos.is_empty() {
            usage();
        }
        let d = rmdf_diff::digest_dir(&dir)?;
        println!("{}", serde_json::to_string_pretty(&d)?);
        return Ok(d.issues.is_empty());
    }
    if pos.len() != 2 {
        usage();
    }
    let summary = rmdf_diff::diff_dirs(&pos[0], &pos[1], max_ex)?;
    rmdf_diff::print_summary(&summary);
    if let Some(p) = json {
        std::fs::write(&p, serde_json::to_vec_pretty(&summary)?)
            .with_context(|| format!("writing {}", p.display()))?;
    }
    Ok(summary.identical)
}
