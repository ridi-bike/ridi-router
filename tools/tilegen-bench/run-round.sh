#!/usr/bin/env bash
# usage: run-round.sh <round> <proto...>   e.g. run-round.sh r0 A B C
set -u
B=$(cd "$(dirname "$0")" && pwd); r=$1; shift
for ds in latvia lithuania slovakia poland; do
  for p in "$@"; do
    reps=1; [[ $ds == latvia || $ds == lithuania ]] && reps=2
    for i in $(seq 1 $reps); do
      echo "== $p-$r $ds run $i"; "$B/run-bench.sh" "$p-$r" "$B/bins/$p-$r" "$ds" | tail -3
    done
  done
  # cross-check prototypes against each other where there is no reference
  if [[ ! -d $B/ref/$ds-0.1 && $# -gt 1 ]]; then
    first=$1
    for p in "${@:2}"; do
      echo "== cross $first-$r vs $p-$r on $ds"
      "$B/tools/rmdf-diff/target/release/rmdf-diff" "$B/out/$first-$r-$ds" "$B/out/$p-$r-$ds" --json "$B/logs/cross-$first-$p-$r-$ds.json" | tail -12
    done
  fi
done
