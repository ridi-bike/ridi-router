#!/usr/bin/env bash
# usage: r2-cross.sh <dataset> [bins...]   (each run under flock, 8G cap, global anchor)
set -uo pipefail
ds=$1; shift
bins=${*:-A-r1 B-r1 C-r1}
B=/home/toms/dev/ridi-router/map-data/bench
E=$B/tmp/C-dev/edge-out
mkdir -p $E
DIFF=$B/tools/rmdf-diff/target/release/rmdf-diff
for b in $bins; do
  out=$E/$b-$ds; tmpd=$E/tmp-$b-$ds; log=$E/$b-$ds.log
  rm -rf $out $tmpd; mkdir -p $out $tmpd
  s=$(date +%s.%N)
  flock $B/heavy.lock systemd-run --user --scope --quiet -p MemoryMax=8G -p MemorySwapMax=2G \
    env RUST_LOG=info TMPDIR=$tmpd RIDI_GRID_ANCHOR=global $B/bins/$b generate-tiles --input $B/inputs/$ds.osm.pbf -o $out --tile-size-deg 0.1 > $log 2>&1
  ec=$?
  e=$(date +%s.%N)
  rm -rf $tmpd
  echo "$b $ds exit=$ec wall=$(python3 -c "print(round($e-$s,1))") tiles=$(find $out -name 'tile_*.rmdf' ! -size 144c | wc -l)"
  [[ $ec != 0 ]] && tail -5 $log | cut -c1-400
done
set -- $bins
first=$1; shift
for b in "$@"; do
  $DIFF $E/$first-$ds $E/$b-$ds --max-examples 5 > $E/diff-$first-$b-$ds.txt 2>&1
  echo "diff $first vs $b: exit=$? $(grep -E 'RESULT' $E/diff-$first-$b-$ds.txt)"
  grep -E "^(files|totals|tiles|points|lines):" $E/diff-$first-$b-$ds.txt
done
