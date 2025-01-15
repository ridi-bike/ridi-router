# Ridi-router - CLI for motorcycle route generation

Ridi-router is a CLI tool for generating motorcycle routes based on your own preferences. Define the type of roads and surfaces you prefer and save that as a rule-file, then use it when generating routes for trips.

## Why

I live in a somewhat rural but densely populated area whith lots of nice forest tracks and paths and unpaved roads, many of which lead to private properties, farms but also many which are for public use.

So if I've got a free hour, I want to spend it riding nice tracks and paths instead of looking at maps trying to find paths and tracks that I can ride.

And so far I've not found an existing tool/app that allows me to define the type of roads I prefer.

So I decided to build a tool that does what I need, offers flexiblity in findind the exact roads/paths/tracks that I like.

## Features

- Round trips - specify start-finish point, direction, approximate distance and get multiple routes that do a loop and bring you back
- Start-finish trips - specify start coordinates and finish coordinates and get multiple route options
- Route statistics - total distance on different road types and surface types, calculates a score for how interesting the route might be (twisty bits vs straight bits)
- Supports input map data from OpenStreetMap.org in either osm.pbf format or json format
- Output route data in gpx or json format

## Output

Generated routes can be saved as json or GPX files. GPX files are a standard that can be used with a lot of different programs and physical GPS devices. For easy viewing https://www.gpxsee.org/ can be used on the desktop or the GPX files can be imported into https://www.gaiagps.com/ for easy sync to mobile devices.

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

Data files for regions can be downloaded at https://download.geofabrik.de/ - there are individual files available for all countries, US states and other special regions. Depending on the region size, these files can be fairly large in their packed state (for example Spain is 1.2 GB, Germany is 4.1 GB, USA is 10.1 GB)

When the files are loaded into ridi-router, they are unpacked and stored in memory in a way that's convenient for route generation, but takes up more memory than the original file by roughly 8-10x, for example Spain would require 9 GB of memory to process.

#### JSON format

Map data json can be downloaded from a web interface at https://overpass-turbo.eu/ by querying the map data based on specific GPS coordinates and distances. This is preferred as it will reduce the file sizes and memory consumption when generating routes.

An example query might look like this. 

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

It queries all relevant data in a 100km zone around a line between two gps points 56.951861,24.113821 and 57.313103,25.281460. The same service is also available as an API endpoint at https://overpass-api.de/api/interpreter that can be queried with `curl` and saved as a json file

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

The above query will produce a json file with the size around 150 MB.

### CLI usage

#### Start-finish route generation

`ridi-router generate-route --input map.json --output routes.gpx --rule-file avoid-pavement.json start-finish --start 56.951861,24.113821 --finish 57.313103,25.281460`

Args:

- input - file to read map data from. Can be either osm.pbf file downloaded form [https://download.geofabrik.de/] or json file downloaded from [https://overpass-api.de/api/interpreter]
- output - a file to write the generated routes to. Can be a gpx file or a json file. Can be omitted for the result to be printed to terminal
- rule-file - a rule file to define route generation options. See below for the format and rule description
- start - GPS coordinates in the format of LAT,LON
- finish - GPS coordinates in the format of LAT,LON

#### Round-trip route generation

`ridi-router generate-route --input map.json --output routes.gpx --rule-file avoid-pavement.json round-trip --start-finish 56.951861,24.113821 --bearing 35 --distance 100000`

Args:

- input - file to read map data from. Can be either osm.pbf file downloaded form [https://download.geofabrik.de/] or json file downloaded from [https://overpass-api.de/api/interpreter]
- output - a file to write the generated routes to. Can be a gpx file or a json file. Can be omitted for the result to be printed to terminal
- rule-file - a rule file to define route generation options. See below for the format and rule description
- start-finish - GPS coordinates in the format of LAT,LON
- bearing - direction specified in degrees where North: 0°, East: 90°, South: 180°, West: 270°
- distance - desired distance for the round trip specified in meters

#### Input Data caching

If the input map file is large and the startup time takes too long, the input map data can be cached in a processed state. This can be done by specifying the `--cache-dir` argument. If this directory is specified, `ridi-router` on first run will cache the input data in the directory and in subsequent runs will read the cached data and considerably speed up the start up time.

Example with data caching
`ridi-router generate-route --input map.json --output routes.gpx --cache-dir ./map-data/cache --rule-file avoid-pavement.json round-trip --start-finish 56.951861,24.113821 --bearing 35 --distance 100000`

### Rule file

A rule file is a json file that is read and used when evaluating which road to take at a given junction. Every junction is evaluated against all basic rules and specified advanced rules.

- basic rules that control the basic navigation like making sure we are going in the right general direction. These rules have built in default values and should only be changed in rare circumstances
- advanced rules add additional checks based on road surface, type and smoothness. These rules should be created based on preferences

Basic rules provide rule criteria and a priority value to use when applying the rule.

Advanced rules can either provide a priority value or specify "avoid" action to prevent the router form picking a road.

Priority values must be between 0 (meaning no priority change) and 255 (highest priority). When a possible road is evaluated, all priority values are summed up from all rules and the one with the highest total priority is picked. If a single "avoid" action is encountered, the road is excluded regardless of the priority values.

An example rule file can be found in `./rule-examples/rules-prefer-unpaved.json` that prefers smaller unpaved roads.

An example rule file that will not pick unpaved roads or paths and trails can be seen here `./rule-examples/rules-avoid-unpaved.json`

Road types, smoothness and surfaces are based on OpenStreetMap.org tag values. Road type is specified as "highway" (https://wiki.openstreetmap.org/wiki/Key:highway), while smoothness (https://wiki.openstreetmap.org/wiki/Key:smoothness) and surface (https://wiki.openstreetmap.org/wiki/Key:surface) are specified as such.

Rule file can be validated against a schema file located in `./rule-examples/schema.json`

#### Basic rules

These rules dictate basic navigation and route finding. Altering these values can lead to broken results but can also help in certain scenarios where geographic obstacles need to be overcome

A rule file with default basic rule settings can be found here `./rule-examples/rules-default.json`

- step_limit - limits the number of steps, defaults to 1'000'000 steps. If this limit is reached, the route variation will be marked abandoned but other variations will continue to be processed
- prefer_same_road - used to stay on the same road for a longer period
- progression_direction - controls how long of a detour can happen before a direction is considered wrong. This can be increased in cases where large obstacles need to be overcome like lakes, rivers without bridges, mountain ranges, etc
- progression_speed - disabled by default. Checks how much progress is made and decides when to stop. Useful in scenarios where geographic obstacles in combination with city streets produce many twists and turns without any significant progress towards the finish
- no_short_detours - avoids jumping off roads at a junction with a more favourable surface or road type just to get back on the same road shortly after for example doing a short detour on a forst track coming off of a primary road just to join back in several hundred meters
- no_sharp_turns - avoids scenarios where missing traffic rules in the OpenStreetMap data cause illegal U turns on highways or off/on ramps

### Advanced usage

#### Server-client setup

Advanced use cases can include a long running server that processes the routes and a client that connects to the server to send and receive route requests. This can be done by running `ridi-router start-server <...args>` and `ridi-router start-client <...args>`. Details on usage are available in the cli help docs.

#### Cache preperation

Cache data files can be prepared for later usage without starting a server or generating routes. This can be done by running `ridi-router prep-cache <...args>`. More info in the cli help docs.

#### Result Debugging

To understand how routes are generated and fine-tune rules, debug information can be enabled and writted to disk. This process slows down route generation and will produce large files with information on each of the steps, junctions and weights that were calcualted on rules.

The debug mode can be enabled by spcifying `--debug-dir`. This directory will be cleared and populated with new debug files each time `generate-routes` command is run.

The debug files can be viewed with the `debug-viewer` build of the `ridi-router` - the debug build can be downloaded from the Github releases or can be built from source by spcifying `--features=debug-viewer`.

Run the debug viewer by doing `ridi-router debug-viewer --debug-dir /path/to/debug/dir`, this will start a local web server on http://0.0.0.0:1337/ which will load the debug files and show a map on the route generation steps.

> [!WARNING]
> The debug viewer is still very much Work In Progress so the functionality is limited and there may still be bugs lurking around.
