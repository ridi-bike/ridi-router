//! Route generation benchmark.
//!
//! route_bench <tiles_dir> sf <lat,lon> <lat,lon> [rules.json]
//! route_bench <tiles_dir> rt <lat,lon> <bearing> <distance_m> [rules.json]
//!
//! Prints one JSON line: wall/cpu seconds, peak RSS, route count, best score.
use std::time::Instant;

use ridi_router_routing::{
    Coords, RouteMode, RouteRequest, RouterRules, RoutingExecutor, RoutingExecutorConfig,
};

fn coords(s: &str) -> Coords {
    let (lat, lon) = s.split_once(',').expect("lat,lon");
    Coords {
        lat: lat.parse().unwrap(),
        lon: lon.parse().unwrap(),
    }
}

fn proc_stat() -> (f64, u64) {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit_once(')').map(|(_, s)| s).unwrap_or("");
    let f: Vec<&str> = after.split_whitespace().collect();
    let ticks = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.);
    let cpu = (ticks(11) + ticks(12)) / 100.;
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let hwm = status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    (cpu, hwm)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let tiles_dir = &args[1];
    let (mode, rules_arg) = match args[2].as_str() {
        "sf" => (
            RouteMode::StartFinish {
                start: coords(&args[3]),
                finish: coords(&args[4]),
            },
            args.get(5),
        ),
        "rt" => (
            RouteMode::RoundTrip {
                start_finish: coords(&args[3]),
                bearing: args[4].parse().unwrap(),
                distance: args[5].parse().unwrap(),
            },
            args.get(6),
        ),
        other => panic!("unknown mode {other}"),
    };
    let rules: RouterRules = match rules_arg {
        Some(p) => serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap(),
        None => RouterRules::default(),
    };

    let t0 = Instant::now();
    let executor = RoutingExecutor::open(RoutingExecutorConfig {
        tiles_dir: tiles_dir.into(),
    })
    .unwrap();
    let open_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let result = executor.generate(RouteRequest { mode, rules }, None);
    let gen_s = t1.elapsed().as_secs_f64();
    let (cpu_s, hwm_kb) = proc_stat();
    let (routes, best, lens) = match &result {
        Ok(c) => {
            let best = c
                .routes
                .iter()
                .map(|r| r.stats.score)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut lens: Vec<u64> = c
                .routes
                .iter()
                .map(|r| (r.stats.len_m / 1000.) as u64)
                .collect();
            lens.sort();
            (c.routes.len(), best, lens)
        }
        Err(e) => {
            eprintln!("error: {e}");
            (0, f64::NAN, vec![])
        }
    };
    println!(
        "{{\"open_s\":{open_s:.3},\"gen_s\":{gen_s:.3},\"cpu_s\":{cpu_s:.2},\"peak_rss_mb\":{},\"routes\":{routes},\"best_score\":{best:.3},\"len_km\":{lens:?}}}",
        hwm_kb / 1024
    );
}
