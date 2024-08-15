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

data-fetch:
  curl --data {{overpass-query}} "https://overpass-api.de/api/interpreter" > map-data/{{map-data-file-name}}

map-data-file-name := "map-data-riga-cesis.json"
# map-data-file-name := "test-map-data-formatted.json"

run-and-load-stdin := 'cat map-data' / map-data-file-name + ' | cargo run -- --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 57.0597507 --to_lon 24.0499688'

# run-and-load-stdin := 'cat map-data' / map-data-file-name + ' | cargo run -- --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 57.31337 --to_lon 25.28080'

# run-and-load-stdin := 'cat map-data' / map-data-file-name + ' | cargo run -- --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 56.8504714 --to_lon 24.2400742'

run-stdin:
  {{run-and-load-stdin}}

run-show-stdin:
  {{run-and-load-stdin}} > map-data/output.gpx
  gpxsee map-data/output.gpx &
  
# run-and-load-file := 'cargo run -- --data_file map-data' / map-data-file-name + ' --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 57.31337 --to_lon 25.28080'

# run-and-load-file := 'cargo run -- --data_file map-data' / map-data-file-name + ' --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 56.8504714 --to_lon 24.2400742'

run-and-load-file := 'cargo run -- --data_file map-data' / map-data-file-name + ' --from_lat 57.1542058021927 --from_lon 24.853520393371586 --to_lat 57.0597507 --to_lon 24.0499688'

run-file:
  {{run-and-load-file}}

run-show-file:
  {{run-and-load-file}} > map-data/output.gpx
  gpxsee map-data/output.gpx &
