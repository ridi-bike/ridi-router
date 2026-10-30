#!/usr/bin/env bash
set -u
B=$(cd "$(dirname "$0")" && pwd); cd $B
D=tools/rmdf-diff/target/release/rmdf-diff
for ds in latvia lithuania slovakia poland; do
  reps=1; [[ $ds == latvia || $ds == lithuania ]] && reps=2
  for i in $(seq 1 $reps); do
    echo "== A-r2 $ds $i"; ./run-bench.sh A-r2 bins/A-r2 $ds | tail -2
    echo "== B-r2 $ds $i"; ./run-bench.sh B-r2 bins/B-r2 $ds RIDI_GRID_ANCHOR=reference | tail -2
  done
done
rm -rf out/*-r2-latvia out/*-r2-lithuania out/*-r2-slovakia out/*-r2-poland
echo "== B-r2 europe"; ./run-bench.sh B-r2 bins/B-r2 europe | tail -2
echo "== diff B-r1-europe vs B-r2-europe"; $D out/B-r1-europe out/B-r2-europe --json logs/diff-europe-r1-r2.json | tail -5
rm -rf out/B-r1-europe out/B-r2-europe
df -h /home | tail -1
echo "== B-r2 world"; ./run-bench.sh B-r2 bins/B-r2 world | tail -2
df -h /home | tail -1
echo ALLDONE
