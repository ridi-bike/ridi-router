#!/usr/bin/env bash
L=/home/toms/dev/ridi-router/map-data/bench/tmp/C-dev/legacy
B=/home/toms/dev/ridi-router/map-data/bench
echo "legacy tile_3600_*: $(command ls $L/ref-fijie | grep -c tile_3600_)  tile_3599_*: $(command ls $L/ref-fijie | grep -c tile_3599_)"
rm -rf $L/a-fijie; mkdir -p $L/a-fijie
systemd-run --user --scope --quiet -p MemoryMax=4G env RUST_LOG=info TMPDIR=$L RIDI_GRID_ANCHOR=reference \
  $B/bins/A-r1 generate-tiles --input $L/fijie.osm.pbf -o $L/a-fijie --tile-size-deg 0.1 > $L/a-fijie.log 2>&1
echo "A exit=$?"
$B/tools/rmdf-diff/target/release/rmdf-diff $L/ref-fijie $L/a-fijie --max-examples 3 2>&1 | grep -E "tiles:|extra_tiles|RESULT" | head -6
