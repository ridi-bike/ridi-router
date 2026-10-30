# Tile generation benchmark harness

Tooling used to compare the tile generation prototypes (October 2026).

- `BRIEF.md` – parity spec every prototype had to meet (the old generator's behaviour)
- `LOCAL_STAGE.md` – interface of the shared per-region local stage
- `EDGE_CASES.md` – antimeridian / high-latitude / southern-hemisphere findings
- `rmdf-diff/` – semantic RMDF tile diff (`cargo build --release`); byte diffs are useless because
  line/tag ordering is not deterministic
- `run-bench.sh` – capped (systemd-run scope) + measured run of one generator binary on one dataset,
  diffed against `ref/<dataset>-0.1`, appended to `results.jsonl`
- `run-round*.sh` – the official benchmark rounds
- `run-fix.sh` – 500 m radius / +180 clamp check: reference-mode parity, global before/after timing and flag diffs
- `edge/` – scripts that built and cross-checked the edge-case datasets

The scripts expect to live in a data directory (we used `map-data/bench/`, gitignored) next to
`inputs/`, `ref/`, `bins/`, `out/`, `logs/`, `tmp/`; copy them there to use them.
