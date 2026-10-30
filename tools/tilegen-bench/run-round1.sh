#!/usr/bin/env bash
set -u
B=$(cd "$(dirname "$0")" && pwd); cd $B
D=tools/rmdf-diff/target/release/rmdf-diff
for ds in latvia lithuania slovakia poland; do
  for p in A B C; do
    reps=1; [[ $ds == latvia || $ds == lithuania ]] && reps=2
    for i in $(seq 1 $reps); do echo "== $p-r1 $ds $i"; ./run-bench.sh $p-r1 bins/$p-r1 $ds | tail -2; done
  done
done
rm -rf out/*-r1-latvia out/*-r1-lithuania out/*-r1-slovakia out/*-r1-poland
echo "== B-r1 europe"; ./run-bench.sh B-r1 bins/B-r1 europe RIDI_GRID_ANCHOR=global | tail -2
echo "== C-r1 europe"; ./run-bench.sh C-r1 bins/C-r1 europe RIDI_GRID_ANCHOR=global | tail -2
echo "== cross B vs C europe"; $D out/B-r1-europe out/C-r1-europe --json logs/cross-B-C-r1-europe.json | tail -6
rm -rf out/C-r1-europe
echo "== A-r1split europe"; ./run-bench.sh A-r1split bins/A-r1-splitbin europe RIDI_GRID_ANCHOR=global | tail -2
echo "== cross B vs Asplit europe"; $D out/B-r1-europe out/A-r1split-europe --json logs/cross-B-A-r1-europe.json | tail -6
rm -rf out/A-r1split-europe
echo ALLDONE
