#!/usr/bin/env bash
# 500 m / +180 fix: parity in reference mode, before/after in global mode.
set -u
cd "$(dirname "$0")"
D=tools/rmdf-diff/target/release/rmdf-diff
for ds in latvia lithuania; do ./run-bench.sh B-fix-ref bins/B-fix $ds RIDI_GRID_ANCHOR=reference; done
for rep in 1 2; do
  for ds in latvia poland; do
    ./run-bench.sh B-r2g$rep bins/B-r2 $ds
    ./run-bench.sh B-fixg$rep bins/B-fix $ds
  done
done
for ds in latvia poland; do $D out/B-r2g1-$ds out/B-fixg1-$ds --json logs/fix-flagdiff-$ds.json --max-examples 5 > logs/fix-flagdiff-$ds.txt 2>&1; done
for e in highlat antimeridian; do
  for b in B-r2 B-fix; do
    rm -rf out/edge-$b-$e; mkdir -p out/edge-$b-$e
    bins/$b generate-tiles --input inputs/edge-$e.osm.pbf -o out/edge-$b-$e --tile-size-deg 0.1 > logs/edge-$b-$e.log 2>&1
  done
  $D out/edge-B-r2-$e out/edge-B-fix-$e --json logs/fix-flagdiff-edge-$e.json --max-examples 5 > logs/fix-flagdiff-edge-$e.txt 2>&1
done
BENCH_KEEP_OUT=0 ./run-bench.sh B-fixg1 bins/B-fix europe
echo DONE
