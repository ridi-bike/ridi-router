#!/usr/bin/env bash
set -euo pipefail
L=/home/toms/dev/ridi-router/map-data/bench/tmp/C-dev/legacy
X=/home/toms/dev/ridi-router/map-data/bench/fixtures/edge
D=/home/toms/dev/ridi-router/map-data/input-world
mkdir -p $X
for ds in fijie fijiw svalbard; do
  cp $L/$ds.osm.pbf $X/$ds.osm.pbf
  rm -rf $X/legacy-$ds; cp -r $L/ref-$ds $X/legacy-$ds
done
for ds in samoa tonga; do
  cp $D/$ds-latest.osm.pbf $X/$ds.osm.pbf
done
du -sh $X; ls $X
