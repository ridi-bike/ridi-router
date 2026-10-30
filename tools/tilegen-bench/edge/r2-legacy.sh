#!/usr/bin/env bash
# Legacy generator (reference) vs C-r1 (reference anchor) on small edge inputs. Run under flock.
set -uo pipefail
B=/home/toms/dev/ridi-router/map-data/bench
D=/home/toms/dev/ridi-router/map-data/input-world
L=$B/tmp/C-dev/legacy
mkdir -p $L
LEG=$B/ref/ridi-router-cli-baseline
DIFF=$B/tools/rmdf-diff/target/release/rmdf-diff
osmium extract --overwrite --no-progress -s simple -b 15.0,78.0,16.6,78.4 $D/norway-latest.osm.pbf -o $L/svalbard.osm.pbf
osmium extract --overwrite --no-progress -s simple -b 179.0,-17.5,180,-16.0 $D/fiji-latest.osm.pbf -o $L/fijie.osm.pbf
osmium extract --overwrite --no-progress -s simple -b -180,-17.5,-179.0,-16.0 $D/fiji-latest.osm.pbf -o $L/fijiw.osm.pbf
for ds in ${DSETS:-svalbard fijie fijiw}; do
  src=$D/$ds-latest.osm.pbf; [[ -f $L/$ds.osm.pbf ]] && src=$L/$ds.osm.pbf
  rm -rf $L/in-$ds $L/ref-$ds $L/c-$ds; mkdir -p $L/in-$ds $L/ref-$ds $L/c-$ds
  ln -s $src $L/in-$ds/$ds.osm.pbf
  systemd-run --user --scope --quiet -p MemoryMax=8G -p MemorySwapMax=2G env RUST_LOG=info TMPDIR=$L \
    $LEG generate-tiles --input-dir $L/in-$ds -o $L/ref-$ds --tile-size-deg 0.1 > $L/ref-$ds.log 2>&1
  echo "$ds legacy exit=$? $(grep -oE 'Grid would require[^"]*|All [0-9]+ PBF files failed[^"]*' $L/ref-$ds.log | head -2 | tr '\n' ' ')"
  systemd-run --user --scope --quiet -p MemoryMax=8G -p MemorySwapMax=2G env RUST_LOG=info TMPDIR=$L RIDI_GRID_ANCHOR=reference \
    $B/bins/C-r1 generate-tiles --input $src -o $L/c-$ds --tile-size-deg 0.1 > $L/c-$ds.log 2>&1
  echo "$ds C-r1 exit=$?"
  if ls $L/ref-$ds/tile_* >/dev/null 2>&1; then
    $DIFF $L/ref-$ds $L/c-$ds --max-examples 5 2>&1 | grep -E "^(files|totals|tiles|points|lines):|RESULT"
  fi
done
