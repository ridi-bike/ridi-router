
overpass-query := '"[out:json];
                    way
                      [highway]
                      [access!=private]
                      [highway!=footway]
                      [motor_vehicle!=private]
                      [motor_vehicle!=no]
                      [!service]
                      [highway!=cycleway]
                      [highway!=steps]
                      [highway!=pedestrian]
                      [access!=no]
                      [highway!=path]
                      [highway!=service]
                      (around:100000,57.15368,24.85370,57.31337,25.28080)->.roads;
                    relation
                      [type=restriction]
                      (around:100000,57.15368,24.85370,57.31337,25.28080)->.rules;
                    (
                      .roads;>>;
                      .rules;>>;
                    );
                    out;"'
run-and-load := 'cat map-data/test-map-data-formatted.json | cargo run -- --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 57.31337 --to_lon 25.28080'

data-fetch:
	curl --data {{overpass-query}} "https://overpass-api.de/api/interpreter" > map-data/test-map-data.json

data-format:
  cat map-data/test-map-data.json | jq | tee map-data/test-map-data-formatted.json

run:
  {{run-and-load}}

run-show:
  {{run-and-load}} > map-data/output.gpx
  gpxsee map-data/output.gpx
  
