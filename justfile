
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
                      [access!=no]
                      [highway!=path]
                      [highway!=service]
                      (around:10,57.15368,24.85370,57.16393,24.89780)->.roads;
                    (.roads;>>;);
                    out;"'
run-and-load := 'cat test-map-data-formatted.json | cargo run'

data-fetch:
	curl --data {{overpass-query}} "https://overpass-api.de/api/interpreter" > test-map-data.json

data-format:
  cat test-map-data.json | jq | tee test-map-data-formatted.json

run:
  {{run-and-load}}

run-show:
  {{run-and-load}} > output.gpx
  gpxsee output.gpx
  
