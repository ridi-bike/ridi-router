# Ridi-router - CLI for motorcycle route generation

Ridi-router is a CLI tool for generating motorcycle routes based on your own rules. Define the type of roads and surfaces you prefer and save that as a rule-file, then use it when generating routes for trips.

## Why

- If I've got a free hour, I want to spend it riding nice tracks and paths instead of looking at maps
- I've not found an existing tool/app that allows me to define the type of roads I prefer

## Features

- Round trips - specify start-finish point, direction, approximate distance and get multiple routes that do a loop and bring you back
- Start-finish trips - specify start coordinates and finish coordinates and get multiple route options
- Route statistics - total distance on different road types and surface types, calculates a score for how interesting the route might be (twisty bits vs straight bits)
- Supports input map data from OpenStreetMap.org in either osm.pbf format or json format
- Output route data in gpx or json format

## How

Run `ridi-router generate-route --input map.json --output routes.gpx --rule-file avoid-pavement.json start-finish --start 56.951861,24.113821 --finish 57.313103,25.281460`

Ridi-router will generate routes based on a naive approximation on how I'd do it manually - start with a point, move in the right direction and at every junction make a decision on which road might be the best option. The best road is evaluated based on multiple rules that can be fine-tuned based on preferences by creating a custom rule-file.

If the chosen road ends up being a dead-end or end up going in the wrong direction, the router will stop, take a step back and try the next best one.

Repeat the process until all possible routes are explored, we've reached an arbitrary step limit or we've reached the finish coordinates.

Multiple different waypoints are chosen to introduce variation in the generated routes.

## Usage

### Get the CLI tool

#### Github Releases

Releases are prepared with binaries for Windows, MacOS and Linux. These can be downloaded form the Github Releases section.

On Windows and MacOS the binaries will be flagged as potentially dangerous. This warning can be ignored.

#### Build from source

Rust must be installed and set up beforehand.

The binary can be built from source by cloning the repo and running `cargo build --release`. A release binary is needed to ensure routes are generated at an acceptable speed.

### Input map data

#### PBF format

Data files for regions can be downloaded at (https://download.geofabrik.de/)[https://download.geofabrik.de/] - there are individual files available for all countries, US states and other special regions. Depending on the region size, these files can be fairly large in their packed state as pbf files and even larger when ridi-router loads them in memory (unpacked state in memory is roughly 5-8x the file size).

#### JSON format

Map data json can be downloaded from a web interface at (https://overpass-turbo.eu/)[https://overpass-turbo.eu/] by querying the data based on specific GPS coordinates. This is preferred as it will reduce the file sizes and memory consumption when generating routes.

An example query might look like this. It queries all relevant data in a 100km zone around a line between two gps points 56.951861,24.113821 and 57.313103,25.281460

```
[out:json];
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
  (around:100000,56.951861,24.113821,57.313103,25.281460)->.roads;
relation
  [type=restriction]
  (around:100000,56.951861,24.113821,57.313103,25.281460)->.rules;
(
  .roads;>>;
  .rules;>>;
);
out;

```

The same service is also available as an API endpoint at (https://overpass-api.de/api/interpreter)[https://overpass-api.de/api/interpreter] that can be queries with `curl` and saved as a json file

```bash
curl --data "[out:json];
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
  (around:100000,56.951861,24.113821,57.313103,25.281460)->.roads;
relation
  [type=restriction]
  (around:100000,56.951861,24.113821,57.313103,25.281460)->.rules;
(
  .roads;>>;
  .rules;>>;
);
out;" "https://overpass-api.de/api/interpreter" > map-data.json

```

### CLI usage

#### Start-finish route generation

`ridi-router generate-route --input map.json --output routes.gpx --rule-file avoid-pavement.json start-finish --start 56.951861,24.113821 --finish 57.313103,25.281460`

Args:

- input - file to read map data from. Can be either osm.pbf file downloaded form (https://download.geofabrik.de/)[https://download.geofabrik.de/] or json file downloaded from (https://overpass-api.de/api/interpreter)[https://overpass-api.de/api/interpreter]https://overpass-turbo.eu/
- output - a file to write the generated routes to. Can be a gpx file or a json file. Can be omitted for the result to be printed to terminal
- rule-file - a rule file to define route generation options. See below for the format and rule description
- start - GPS coordinates in the format of LAT,LON
- finish - GPS coordinates in the format of LAT,LONhttps://overpass-turbo.eu/<t_��>ýrequire"cmp.utilshttps://overpass-turbo.eu/.feedkeys".run(603)
-

#### Round-trip route generation

`ridi-router generate-route --input map.json --output routes.gpx --rule-file avoid-pavement.json round-trip --start-finish 56.951861,24.113821 --bearing 35 --distance 100000`

Args:

- input - file to read map data from. Can be either osm.pbf file downloaded form (https://download.geofabrik.de/)[https://download.geofabrik.de/] or json file downloaded from (https://overpass-api.de/api/interpreter)[https://overpass-api.de/api/interpreter]
- output - a file to write the generated routes to. Can be a gpx file or a json file. Can be omitted for the result to be printed to terminal
- rule-file - a rule file to define route generation options. See below for the format and rule description
- start-finish - GPS coordinates in the format of LAT,LON
- bearing - direction specified in degrees where North: 0°, East: 90°, South: 180°, West: 270°
- distance - desired distance for the round trip specified in meters

#### Input Data caching

If the input map file is large and the startup time takes too long, the input map data can be cached in a processed state. This can be done by specifying the `--cache-dir` argument. If this directory is specified, `ridi-router` on first run will cache the input data in the directory and in subsequent runs will read the cached data and considerably speed up the start up time.

Example with data caching
`ridi-router generate-route --input map.json --output routes.gpx --cache-dir ./map-data/cache --rule-file avoid-pavement.json round-trip --start-finish 56.951861,24.113821 --bearing 35 --distance 100000`
