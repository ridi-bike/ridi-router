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

## Breaking Changes in v1.0

**IMPORTANT**: Version 1.0 introduces a new tile-based map data format (RMDF) and removes several deprecated features:

- **Removed**: JSON input support (only PBF files supported)
- **Removed**: Bincode cache system (`--cache-dir` parameter)
- **Removed**: `prep-cache` command
- **Removed**: Direct PBF loading for routing (`--input` parameter for `generate-route`)
- **Required**: You must now generate RMDF tiles before routing using the `generate-tiles` command
- **New**: `--tiles` directory parameter (required for routing)

**Migration**: To upgrade from v0.x, you'll need to:
1. Download OSM PBF files for your region from https://download.geofabrik.de/
2. Generate RMDF tiles using `ridi-router generate-tiles`
3. Use the generated tiles directory for routing

## Usage

### Get the CLI tool

#### Github Releases

Releases are prepared with binaries for Windows, MacOS and Linux. These can be downloaded form the Github Releases section.

On Windows and MacOS the binaries will be flagged as potentially dangerous. This warning can be ignored.

#### Build from source

Rust must be installed and set up beforehand.

The binary can be built from source by cloning the repo and running `cargo build --release`. A release binary is needed to ensure routes are generated at an acceptable speed.

### Input Map Data (PBF format)

Data files for regions can be downloaded at https://download.geofabrik.de/ - there are individual files available for all countries, US states and other special regions. Depending on the region size, these files can be fairly large in their packed state (for example Spain is 1.2 GB, Germany is 4.1 GB, USA is 10.1 GB).

**Note**: As of version 1.0, only PBF format is supported. JSON format has been removed.

### CLI Usage

#### Step 1: Generate Tiles

Before routing, you must first generate RMDF tiles from a PBF file:

```bash
ridi-router generate-tiles \
    --input montenegro.osm.pbf \
    --output ./tiles \
    --tile-size 0.1
```

Args:
- `--input` - OSM PBF file downloaded from https://download.geofabrik.de/
- `--output` - Directory where tiles and manifest will be stored
- `--tile-size` - Tile size in degrees (default: 1.0, smaller values like 0.1 for better granularity)

#### Step 2: Start-Finish Route Generation

```bash
ridi-router generate-route \
    --tiles ./tiles \
    --output routes.gpx \
    --rule-file avoid-pavement.json \
    start-finish \
    --start 56.951861,24.113821 \
    --finish 57.313103,25.281460
```

Args:
- `--tiles` - Directory containing RMDF tiles and manifest.json (from Step 1)
- `--output` - File to write the generated routes to (GPX or JSON). Can be omitted to print to terminal
- `--rule-file` - Rule file defining route generation options (see below)
- `--start` - Start GPS coordinates (LAT,LON)
- `--finish` - Finish GPS coordinates (LAT,LON)

#### Step 2 Alternative: Round-Trip Route Generation

```bash
ridi-router generate-route \
    --tiles ./tiles \
    --output routes.gpx \
    --rule-file avoid-pavement.json \
    round-trip \
    --start-finish 56.951861,24.113821 \
    --bearing 35 \
    --distance 100000
```

Args:
- `--tiles` - Directory containing RMDF tiles and manifest.json
- `--output` - File to write the generated routes to (GPX or JSON)
- `--rule-file` - Rule file defining route generation options
- `--start-finish` - Start and finish GPS coordinates (LAT,LON)
- `--bearing` - Direction in degrees (North: 0°, East: 90°, South: 180°, West: 270°)
- `--distance` - Desired round trip distance in meters

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

- step_limit - limits the number of steps, defaults to 30'000 steps. If this limit is reached, the route variation will be marked abandoned but other variations will continue to be processed
- prefer_same_road - used to stay on the same road for a longer period
- progression_direction - controls how long of a detour can happen before a direction is considered wrong. This can be increased in cases where large obstacles need to be overcome like lakes, rivers without bridges, mountain ranges, etc
- progression_speed - disabled by default. Checks how much progress is made and decides when to stop. Useful in scenarios where geographic obstacles in combination with city streets produce many twists and turns without any significant progress towards the finish
- no_short_detours - avoids jumping off roads at a junction with a more favourable surface or road type just to get back on the same road shortly after for example doing a short detour on a forst track coming off of a primary road just to join back in several hundred meters
- no_sharp_turns - avoids scenarios where missing traffic rules in the OpenStreetMap data cause illegal U turns on highways or off/on ramps

### Advanced Usage

#### Result Debugging

To understand how routes are generated and fine-tune rules, debug information can be enabled and writted to disk. This process slows down route generation and will produce large files with information on each of the steps, junctions and weights that were calcualted on rules.

The debug mode can be enabled by spcifying `--debug-dir`. This directory will be cleared and populated with new debug files each time `generate-routes` command is run.

The debug files can be viewed with the `debug-viewer` build of the `ridi-router` - the debug build can be downloaded from the Github releases or can be built from source by spcifying `--features=debug-viewer`.

Run the debug viewer by doing `ridi-router debug-viewer --debug-dir /path/to/debug/dir`, this will start a local web server on http://0.0.0.0:1337/ which will load the debug files and show a map on the route generation steps.

> [!WARNING]
> The debug viewer is still very much Work In Progress so the functionality is limited and there may still be bugs lurking around.
