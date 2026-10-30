#!/usr/bin/env bash
# Build edge-case datasets. Run under flock.
set -euo pipefail
D=/home/toms/dev/ridi-router/map-data/input-world
O=/home/toms/dev/ridi-router/map-data/bench/inputs
T=/home/toms/dev/ridi-router/map-data/bench/tmp/C-dev/edge
mkdir -p $T
X="osmium extract --overwrite --no-progress -s complete_ways"

# antimeridian: Pacific islands + us-pacific (Alaska/Aleutians, Hawaii) + Chukotka both sides of 180
$X -b 160,60,180,72 $D/far-eastern-fed-district-latest.osm.pbf -o $T/chuk-e.pbf
$X -b -180,60,-165,72 $D/far-eastern-fed-district-latest.osm.pbf -o $T/chuk-w.pbf
osmium merge --overwrite --no-progress -o $O/edge-antimeridian.osm.pbf \
  $D/fiji-latest.osm.pbf $D/kiribati-latest.osm.pbf $D/tuvalu-latest.osm.pbf $D/tonga-latest.osm.pbf \
  $D/samoa-latest.osm.pbf $D/wallis-et-futuna-latest.osm.pbf $D/american-oceania-latest.osm.pbf \
  $D/us-pacific-latest.osm.pbf $T/chuk-e.pbf $T/chuk-w.pbf

# southern/western hemisphere
cp $D/new-zealand-latest.osm.pbf $O/edge-nz.osm.pbf
cp $D/chile-latest.osm.pbf $O/edge-chile.osm.pbf

# high latitude: northern Norway + Svalbard, Greenland, northern Nunavut
$X -b -12,67,36,82 $D/norway-latest.osm.pbf -o $T/norway-n.pbf
$X -b -121,68,-49,85 $D/nunavut-latest.osm.pbf -o $T/nunavut-n.pbf
osmium merge --overwrite --no-progress -o $O/edge-highlat.osm.pbf $T/norway-n.pbf $T/nunavut-n.pbf $D/greenland-latest.osm.pbf

# very dense city region: Kanto
$X -b 138.9,34.85,140.95,36.5 $D/japan-latest.osm.pbf -o $O/edge-kanto.osm.pbf

# huge / complex residential & military polygons
osmium merge --overwrite --no-progress -o $O/edge-polygons.osm.pbf $D/israel-and-palestine-latest.osm.pbf $D/north-korea-latest.osm.pbf

rm -rf $T
ls -la $O/edge-*
for f in $O/edge-*.osm.pbf; do echo "$f $(osmium fileinfo -e -g data.bbox $f) nodes=$(osmium fileinfo -e -g data.count.nodes $f)"; done
