set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Common local paths / example inputs
pbf-latvia := "./map-data/pbf/latvia-latest.osm.pbf"
pbf-spain := "./map-data/pbf/spain-latest.osm.pbf"
pbf-greece := "./map-data/pbf/greece-latest.osm.pbf"
tiles-dir := "./map-data/output"
routes-dir := "./map-data/routes"
rule-file := "./rule-examples/rules-prefer-unpaved.json"
start := "56.951861,24.113821"
finish := "57.313103,25.281460"
round-trip-start := "56.951861,24.113821"
round-trip-bearing := "35"
round-trip-distance := "100000"

rule-schema:
	cargo run --features=rule-schema-writer -- rule-schema-write --destination rule-examples/schema.json

data-fetch-pbf-latvia:
	mkdir -p ./map-data/pbf
	wget -O {{pbf-latvia}} https://download.geofabrik.de/europe/latvia-latest.osm.pbf

data-fetch-pbf-spain:
	mkdir -p ./map-data/pbf
	wget -O {{pbf-spain}} https://download.geofabrik.de/europe/spain-latest.osm.pbf

data-fetch-pbf-greece:
	mkdir -p ./map-data/pbf
	wget -O {{pbf-greece}} https://download.geofabrik.de/europe/greece-latest.osm.pbf

generate-tiles-latvia:
	cargo run -- generate-tiles --input {{pbf-latvia}} --output {{tiles-dir}} --tile-size-deg 0.1

generate-route-json:
	rm -rf {{routes-dir}}
	cargo run -- generate-route --tiles {{tiles-dir}} --output-dir {{routes-dir}} --format json --rule-file {{rule-file}} start-finish --start {{start}} --finish {{finish}}

generate-route-gpx:
	rm -rf {{routes-dir}}
	cargo run -- generate-route --tiles {{tiles-dir}} --output-dir {{routes-dir}} --format gpx --rule-file {{rule-file}} start-finish --start {{start}} --finish {{finish}}

generate-round-trip-json:
	rm -rf {{routes-dir}}
	cargo run -- generate-route --tiles {{tiles-dir}} --output-dir {{routes-dir}} --format json --rule-file {{rule-file}} round-trip --start-finish {{round-trip-start}} --bearing {{round-trip-bearing}} --distance {{round-trip-distance}}

list-route-files:
	ls -la {{routes-dir}}

open-gpx-routes:
	gpxsee {{routes-dir}}/*.gpx

rmdf-viewer:
	cargo run --features rmdf-viewer -- rmdf-viewer --input-dir {{tiles-dir}}
