types-gen:
	typeshare ./src --lang=typescript --output-file=./src/debug/viewer/ui/api-types.ts

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
	cargo run --release -- dual --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}} --input map-data/{{map-data-json-name}} --output map-data/output.gpx --rule-file map-data/rules-empty.json
	gpxsee map-data/output.gpx &

run-load-pbf-show:
	cargo run --release -- dual --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}} --input map-data/latvia-latest.osm.pbf --output map-data/output.gpx --rule-file map-data/rules-empty.json
	gpxsee map-data/output.gpx &

run-load-cache-show:
	cargo run --release -- dual --start {{gps-test-from-lat}},{{gps-test-from-lon}} --finish {{gps-test-to-lat}},{{gps-test-to-lon}} --input map-data/latvia-latest.osm.pbf --output map-data/output.gpx --cache-dir map-data/cache/latvia --rule-file map-data/rules-prefer-unpaved.json
	gpxsee map-data/output.gpx &

run-gr:
	cargo run --release -- dual --input ./map-data/greece-latest.osm.pbf --output map-data/gr.gpx --cache-dir ./map-data/cache/greece start-finish --start 37.0458401,22.1265497 --finish 37.0744365,22.4263953

run-gr-short:
	cargo run --release -- dual --start 37.0331605,22.1573558 --finish 37.041196,22.182086 --input ./map-data/greece-latest.osm.pbf --output map-data/gr.gpx --cache-dir ./map-data/cache/greece 

run-lv-round:
	cargo run --release -- dual --debug-dir ./map-data/debug --input /map-data/latvia-latest.osm.pbf --output map-data/lv.gpx --cache-dir ./map-data/cache/latvia --rule-file map-data/rules-prefer-unpaved.json round-trip --center {{gps-test-from-lat}},{{gps-test-from-lon}} --bearing 0 --distance 100000

cache-lv:
	cargo run --release -- cache --input ./map-data/latvia-latest.osm.pbf --cache-dir ./map-data/cache/latvia

cache-spain:
	cargo run --release -- cache --input ./map-data/spain-latest.osm.pbf --cache-dir ./map-data/cache/spain
