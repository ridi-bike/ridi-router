 I want to redesign the way map data is loaded, stored and processed. I want to replace the current binary format that is serialized and deserialized via the bincode package with a
  memory mapped file format.

  it must do the following
  - the new data format must serve two purposes - the existing routing algorithm as well as visually showing/drawing an actual map with lots of details
  - for the purposes of the routing algorthm the datashould closely match the data currently being used by the routing algorithm (lines, points, tags, nogo, etc) to limit the code
  changes required in the routing algorithm
  - it must allow for tiling - creating individual small map files with a certain section of the globe's map and use these tiles either individually or combine them
  - the individual tiles must be combined transparently and the routing algorithm should not be aware when it is reacing a new tile or crossing a border into a new tile. it must
  also handle missing tiles (when a subset of tiles available)

  there will need to be two main parts of the change
  - ridi-map-data-format (*.rmdf) tile generation from openstreetmap pbf files
  - reading and memory mapping these rmdf files for routing

  the map generation will include two main parts
  - existing data with the extended analysis around nogo areas, residential proximity, etc
  - other map data required only for map drawing (building shapes, natural features, foot paths, points of interests, shops, petrol stations, other amenities, etc)

  the data generation must be configurable in these ways
  - tile size (we will start by creating tiles if 1 degree x 1 degree for simplcity)
  - level of detail included for drawing maps

  the map drawing is out of scope for this feature.


   There are a few additions and changes
  - the system should not attempt to handle tiles of different sizes at the same time. for generation the ouput location must be cleared and a clean tile set must be written. if
  reading the headers must be checked so that tiles are laoded of matching size and version

  - the rmdf must maintain a data structure for finding nearby nodes to a specific gps coordinates. if the user chooses a gps point, the closest suitable node lookup must be
  peformant (can't loop though all nodes and calc distance)

  - there should be no limit on the number of tiles to be loaded for a routing request - if the tiles are configured to be very small, a routing request may cross 1000s of tiles

  - there should be no file size limits to tiles, the tile file size will be controlled via manually setting the right tile size in degrees

  - there is no reason to maintain the existing pbf or json reading in the state they are in, nor the exsitng binary caching format. moving forward the new rmdf files will be only
  ones in use and no backward compatibility is required

  - one other thing to note is that this router will eventually be prepared for use on ios, android and wasm in addition to the existing targets. in the ios and android the memory
  mapped reading must work as described. but the memory mapping should be abstracted in a way where in wasm it can be swapped out for some other mechanism (s3 hosted tiles with http
  range reads for specific offsets?? unclear, to be determined). but the main thing is - the rmdf file reading and memory mapping should be abstracted. if this is not feasable,
  these concerns should be discussed, all possible solutions evaluated for an acceptable solution

  - a tile file count of > 1M is not unreasonable, this may be an acceptable tradeoff when investigating performance vs file sizes vs tile sizes:w
