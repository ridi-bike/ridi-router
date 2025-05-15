types-gen:
	typeshare ./src --lang=typescript --output-file=./src/debug/viewer/ui/src/api-types.ts
	cargo run --features=rule-schema-writer -- rule-schema-write --destination rule-examples/schema.json

gps-query-range := '100000' # 100km
gps-query-from := '56.951861,24.113821' # riga
gps-query-to := '57.313103,25.281460' # cesis
map-data-json-name := "map-data-riga-cesis.json"

# gps-query-range := '100' # 100m
# gps-query-from := '57.155453,24.853327' # sigulda
# gps-query-to := '57.155453,24.853327' # sigulda
# map-data-json-name := "test-data-sig-100.json"

overpass-query := '"[out:json];
                    way
                      [highway]
                      [highway!=cycleway]
                      [highway!=steps]
                      [highway!=pedestrian]
                      [highway!=path]
                      [highway!=service]
                      [highway!=footway]
                      [motor_vehicle!=private]
                      [motor_vehicle!=no]
                      [!service]
                      [access!=no]
                      [access!=private]
                      (around:' + gps-query-range + ',' + gps-query-from + ',' + gps-query-to + ')->.roads;
                    relation
                      [type=restriction]
                      (around:' + gps-query-range + ',' + gps-query-from + ',' + gps-query-to + ')->.rules;
                    (
                      .roads;>>;
                      .rules;>>;
                    );
                    out;"'

data-fetch-overpass-query:
  curl --data {{overpass-query}} "https://overpass-api.de/api/interpreter" > map-data/{{map-data-json-name}}

data-fetch-pbf-latvia:
	wget -O map-data/latvia-latest.osm.pbf https://download.geofabrik.de/europe/latvia-latest.osm.pbf 

data-fetch-pbf-spain:
	wget -O map-data/spain-latest.osm.pbf https://download.geofabrik.de/europe/spain-latest.osm.pbf 

data-fetch-pbf-greece:
	wget -O map-data/greece-latest.osm.pbf https://download.geofabrik.de/europe/greece-latest.osm.pbf 

# gps-test-from-lat := '56.92517' # zaķusala
# gps-test-from-lon := '24.13688' # zaķusala
# gps-test-from-lat := '57.55998' # zilaiskalns
# gps-test-from-lon := '25.20804' # zilaiskalns
gps-test-from-lat := '57.154260' # sigulda
gps-test-from-lon := '24.853496' # sigulda
# gps-test-from-lat := '36.618195' # malaga
# gps-test-from-lon := '-4.500159' # malaga
gps-test-to-lat := '56.856551'		# doles sala
gps-test-to-lon := '24.253038'		# doles sala
# gps-test-to-lat := '57.111708'		# garciems
# gps-test-to-lon := '24.192656'		# garciems
# gps-test-to-lat := '56.62557'		# garoza
# gps-test-to-lon := '23.93226'		# garoza
# gps-test-to-lat := '37.119409'		# gergal, spain
# gps-test-to-lon := '-2.541200'		# gergal, spain

run-load-json-show:
	cargo run -- generate-route --input map-data/{{map-data-json-name}} --output map-data/output.gpx --rule-file rule-examples/rules-empty.json start-finish --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}}
	gpxsee map-data/output.gpx &

run-load-pbf-show:
	cargo run -- generate-route --input map-data/latvia-latest.osm.pbf --output map-data/output.gpx --rule-file rule-examples/rules-empty.json start-finish --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}}
	gpxsee map-data/output.gpx &

run-load-cache-show:
	cargo run -- generate-route --input map-data/latvia-latest.osm.pbf --output map-data/output.gpx --cache-dir map-data/cache/latvia --rule-file rule-examples/rules-prefer-unpaved.json start-finish --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}}
	gpxsee map-data/output.gpx &

run-gr:
	cargo run -- generate-route --input ./map-data/greece-latest.osm.pbf --output map-data/gr.gpx --cache-dir ./map-data/cache/greece start-finish --start 37.0458401,22.1265497 --finish 37.0744365,22.4263953

run-gr-short:
	cargo run -- generate-route --input ./map-data/greece-latest.osm.pbf --output map-data/gr.gpx --cache-dir ./map-data/cache/greece start-finish --start 37.0331605,22.1573558 --finish 37.041196,22.182086 

run-lv-round-debug:
	cargo run -- generate-route --debug-dir ./map-data/debug --input ./map-data/latvia-latest.osm.pbf --output map-data/lv.gpx --cache-dir ./map-data/cache/latvia --rule-file rule-examples/rules-prefer-unpaved.json round-trip --start-finish {{gps-test-from-lat}},{{gps-test-from-lon}} --bearing 0 --distance 100000

run-lv-round:
	cargo run -- generate-route --input ./map-data/latvia-latest.osm.pbf --output map-data/lv.gpx --cache-dir ./map-data/cache/latvia --rule-file rule-examples/rules-prefer-unpaved.json round-trip --start-finish {{gps-test-from-lat}},{{gps-test-from-lon}} --bearing 0 --distance 100000

run-lv-round-dead-ends:
	cargo run -- generate-route --input ./map-data/latvia-latest.osm.pbf --output map-data/lv.gpx --cache-dir ./map-data/cache/latvia --rule-file rule-examples/rules-prefer-unpaved.json --selected-outputs=route-split round-trip --start-finish {{gps-test-from-lat}},{{gps-test-from-lon}} --bearing 0 --distance 100000

run-lv-server:
  cargo run -- start-server --input ./map-data/latvia-latest.osm.pbf --cache-dir ./map-data/cache/latvia --socket-name lv

run-lv-client:
  cat ./map-data/lv-rules.json | cargo run -- start-client --socket-name lv --route-req-id 2uksfxTDrJs0LF1bhcMAcDHOkgg round-trip --start-finish=56.58266,25.03892 --bearing=0 --distance=100000

run-nl-server:
  cargo run -- start-server --input ./map-data/netherlands-latest.osm.pbf --cache-dir ./map-data/cache/netherlands --socket-name nl

run-nl-client:
  cat ./map-data/nl-rules.json | cargo run -- start-client --socket-name nl --route-req-id 2uksfxTDrJs0LF1bhcMAcDHOkgg round-trip --start-finish=52.02105,4.44122 --bearing=105 --distance=100000

run-lv-gen:
  cargo run -- generate-route --debug-dir ./map-data/debug --input ./map-data/latvia-latest-apps.osm.pbf --cache-dir ./map-data/cache/latvia --rule-file ./rules.json start-finish --start 57.170998,24.86442 --finish 56.64119,24.48387

cache-lv:
	cargo run -- prep-cache --input ./map-data/latvia-latest.osm.pbf --cache-dir ./map-data/cache/latvia

cache-spain:
	cargo run -- prep-cache --input ./map-data/spain-latest.osm.pbf --cache-dir ./map-data/cache/spain

debug-viewer:
	cargo run --features debug-viewer -- debug-viewer --debug-dir ./map-data/debug
