#!/usr/bin/env bash
# Benchmark one tile-generator binary on one dataset.
#
# usage: run-bench.sh <label> <binary> <dataset> [KEY=VALUE ...]
#   dataset: latvia | lithuania | slovakia | poland  (input: inputs/<dataset>.osm.pbf)
#   KEY=VALUE: extra environment for the generator (e.g. RAYON_NUM_THREADS=4)
#
# Env overrides: BENCH_MEMORY_MAX (default 20G), BENCH_SWAP_MAX (default 8G),
#                BENCH_TILE_SIZE (default 0.1), BENCH_KEEP_OUT=0 to delete output after diffing.
#
# Produces: out/<label>-<dataset>/ (tiles), logs/<label>-<dataset>.log (generator log),
#           logs/<label>-<dataset>.diff.{txt,json} (if ref/<dataset>-0.1 exists),
#           one JSON line appended to results.jsonl.
set -uo pipefail

if [[ $# -lt 3 ]]; then
  sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
fi

label=$1 binary=$2 dataset=$3
shift 3
extra_env=("$@")
for kv in "${extra_env[@]}"; do
  [[ $kv == *=* ]] || { echo "extra args must be KEY=VALUE, got: $kv" >&2; exit 2; }
done

case $dataset in latvia|lithuania|slovakia|poland|europe|world) ;; *) echo "unknown dataset: $dataset" >&2; exit 2;; esac

BENCH=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
binary=$(realpath "$binary") || { echo "binary not found" >&2; exit 2; }
[[ -x $binary ]] || { echo "binary not executable: $binary" >&2; exit 2; }
input="$BENCH/inputs/$dataset.osm.pbf"
[[ -e $input ]] || { echo "missing input $input" >&2; exit 2; }

tile_size=${BENCH_TILE_SIZE:-0.1}
mem_max=${BENCH_MEMORY_MAX:-20G}
swap_max=${BENCH_SWAP_MAX:-8G}
name="$label-$dataset"
out="$BENCH/out/$name"
tmpd="$BENCH/tmp/$name"
log="$BENCH/logs/$name.log"
stats="$BENCH/logs/$name.stats"
pidfile="$BENCH/logs/$name.pid"
results="$BENCH/results.jsonl"
ref="$BENCH/ref/$dataset-0.1"
diff_bin="$BENCH/tools/rmdf-diff/target/release/rmdf-diff"

mkdir -p "$BENCH/out" "$BENCH/logs" "$BENCH/tmp"
rm -rf "$out" "$tmpd"
mkdir -p "$out" "$tmpd"
rm -f "$stats" "$pidfile"

# Optional: git commit of the worktree the binary was built in (<worktree>/target/<profile>/bin).
git_commit=null git_dirty=null
if [[ $binary == */target/* ]]; then
  wt=${binary%%/target/*}
  if c=$(git -C "$wt" rev-parse HEAD 2>/dev/null); then
    git_commit="\"$c\""
    if [[ -n $(git -C "$wt" status --porcelain --untracked-files=no 2>/dev/null) ]]; then git_dirty=true; else git_dirty=false; fi
  fi
fi

# Runs inside the scope: starts the generator as a child, records its PID, waits, then reads
# the scope cgroup's counters before the scope disappears.
read -r -d '' WRAPPER <<'EOF'
stats=$1 pidfile=$2; shift 2
cg=/sys/fs/cgroup$(sed -n 's/^0:://p' /proc/self/cgroup)
echo "cgroup=$cg" > "$stats"
"$@" &
pid=$!
echo "$pid" > "$pidfile"
wait "$pid"; ec=$?
{
  echo "exit_code=$ec"
  echo "memory_peak=$(cat "$cg/memory.peak" 2>/dev/null)"
  echo "swap_peak=$(cat "$cg/memory.swap.peak" 2>/dev/null)"
  awk '{print "cpu_" $1 "=" $2}' "$cg/cpu.stat" 2>/dev/null
  awk '{print "memevents_" $1 "=" $2}' "$cg/memory.events" 2>/dev/null
} >> "$stats"
exit $ec
EOF

echo "[run-bench] $name: $binary generate-tiles --input $input -o $out --tile-size-deg $tile_size ${extra_env[*]}"
echo "[run-bench] MemoryMax=$mem_max MemorySwapMax=$swap_max TMPDIR=$tmpd log=$log"

start_ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
t0=$(date +%s.%N)
systemd-run --user --scope --quiet --collect \
  --unit="bench-${name//[^A-Za-z0-9_-]/_}-$$" \
  -p MemoryMax="$mem_max" -p MemorySwapMax="$swap_max" -p OOMPolicy=continue \
  bash -c "$WRAPPER" wrapper "$stats" "$pidfile" \
  env RUST_LOG=info TMPDIR="$tmpd" "${extra_env[@]}" \
  "$binary" generate-tiles --input "$input" -o "$out" --tile-size-deg "$tile_size" \
  >"$log" 2>&1 &
outer=$!

# Sample VmHWM of the real generator process (PID written by the wrapper) every second.
gen_pid="" hwm_kb=0 rss_samples=0
while kill -0 "$outer" 2>/dev/null; do
  if [[ -z $gen_pid && -s $pidfile ]]; then gen_pid=$(<"$pidfile"); fi
  if [[ -n $gen_pid ]]; then
    v=$(awk '/^VmHWM:/{print $2}' "/proc/$gen_pid/status" 2>/dev/null)
    if [[ -n $v ]]; then
      (( v > hwm_kb )) && hwm_kb=$v
      rss_samples=$((rss_samples + 1))
    fi
  fi
  if [[ -z $gen_pid ]]; then sleep 0.1; else sleep 1; fi
done
wait "$outer"; outer_ec=$?
t1=$(date +%s.%N)
wall=$(awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.3f", b-a}')

get() { sed -n "s/^$1=//p" "$stats" 2>/dev/null | head -1; }
exit_code=$(get exit_code); exit_code=${exit_code:-$outer_ec}
mem_peak=$(get memory_peak); swap_peak=$(get swap_peak)
user_us=$(get cpu_user_usec); sys_us=$(get cpu_system_usec)
oom_kills=$(get memevents_oom_kill)
cgroup=$(get cgroup)
if [[ -n $oom_kills ]]; then
  (( oom_kills > 0 )) && oom=true || oom=false
else
  [[ $exit_code == 137 ]] && oom=true || oom=false   # fallback: SIGKILL
fi

out_bytes=$(du -sb "$out" | cut -f1)
tiles=$(find "$out" -maxdepth 1 -name 'tile_*.rmdf' | wc -l)
# Empty tiles (header + checksum only) are exactly 144 bytes.
tiles_nonempty=$(find "$out" -maxdepth 1 -name 'tile_*.rmdf' ! -size 144c | wc -l)
manifest=$([[ -f $out/manifest.json ]] && echo true || echo false)

diff_json=null
if [[ -d $ref && $exit_code == 0 ]]; then
  if [[ ! -x $diff_bin ]]; then
    (cd "$BENCH/tools/rmdf-diff" && cargo build --release -q) || true
  fi
  if [[ -x $diff_bin ]]; then
    "$diff_bin" "$ref" "$out" --json "$BENCH/logs/$name.diff.json" --max-examples 5 \
      > "$BENCH/logs/$name.diff.txt" 2>&1
    diff_ec=$?
    if [[ -s $BENCH/logs/$name.diff.json ]]; then
      diff_json=$(jq -c --argjson ec "$diff_ec" \
        '{exit_code: $ec, identical, tiles, diffs, candidate_issues, ref_issues, elapsed_ms}' \
        "$BENCH/logs/$name.diff.json")
    else
      diff_json=$(jq -nc --argjson ec "$diff_ec" '{exit_code: $ec, error: "no json output"}')
    fi
    sed -n '/^examples:/q;p' "$BENCH/logs/$name.diff.txt"
  fi
fi

num() { [[ -n ${1:-} ]] && echo "$1" || echo null; }
line=$(jq -nc \
  --arg label "$label" --arg dataset "$dataset" --arg binary "$binary" --arg ts "$start_ts" \
  --arg input "$(realpath "$input")" --arg out "$out" --arg log "$log" --arg cgroup "$cgroup" \
  --arg mem_max "$mem_max" --arg swap_max "$swap_max" --arg tile_size "$tile_size" \
  --argjson env "$(printf '%s\n' "${extra_env[@]}" | jq -Rsc 'split("\n") | map(select(length>0))')" \
  --argjson git_commit "$git_commit" --argjson git_dirty "$git_dirty" \
  --argjson wall "$wall" --argjson exit_code "$exit_code" --argjson oom "$oom" \
  --argjson user_us "$(num "$user_us")" --argjson sys_us "$(num "$sys_us")" \
  --argjson hwm_kb "$hwm_kb" --argjson rss_samples "$rss_samples" \
  --argjson mem_peak "$(num "$mem_peak")" --argjson swap_peak "$(num "$swap_peak")" \
  --argjson oom_kills "$(num "$oom_kills")" \
  --argjson out_bytes "$out_bytes" --argjson tiles "$tiles" --argjson tiles_nonempty "$tiles_nonempty" \
  --argjson manifest "$manifest" --argjson diff "$diff_json" \
  '{timestamp: $ts, label: $label, dataset: $dataset, binary: $binary, git_commit: $git_commit,
    git_dirty: $git_dirty, input: $input, tile_size_deg: ($tile_size|tonumber), extra_env: $env,
    memory_max: $mem_max, swap_max: $swap_max,
    exit_code: $exit_code, oom_killed: $oom, oom_kill_events: $oom_kills,
    wall_s: $wall,
    user_s: (if $user_us then $user_us/1e6 else null end),
    sys_s: (if $sys_us then $sys_us/1e6 else null end),
    cpu_s: (if $user_us and $sys_us then ($user_us+$sys_us)/1e6 else null end),
    peak_rss_bytes: ($hwm_kb*1024), peak_rss_samples: $rss_samples,
    cgroup_memory_peak_bytes: $mem_peak, cgroup_swap_peak_bytes: $swap_peak,
    output_bytes: $out_bytes, tile_files: $tiles, tile_files_nonempty: $tiles_nonempty,
    manifest_present: $manifest, diff: $diff, out_dir: $out, log: $log, cgroup: $cgroup}')
echo "$line" >> "$results"
echo "$line" | jq '{label, dataset, exit_code, oom_killed, wall_s, cpu_s, peak_rss_gb: (.peak_rss_bytes/1e9),
  cgroup_peak_gb: (if .cgroup_memory_peak_bytes then .cgroup_memory_peak_bytes/1e9 else null end),
  swap_peak_gb: (if .cgroup_swap_peak_bytes then .cgroup_swap_peak_bytes/1e9 else null end),
  output_mb: (.output_bytes/1e6), tile_files, tile_files_nonempty, diff_identical: .diff.identical}'

if [[ ${BENCH_KEEP_OUT:-1} == 0 ]]; then rm -rf "$out"; fi
rm -rf "$tmpd"
rm -f "$pidfile"
exit "$exit_code"
