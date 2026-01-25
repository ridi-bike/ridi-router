currentl the `generate-tiles` action works as follows.
- find pbf file min and max lat, lon
- calculate tiles covered by the pbf file
- parallellize tile generateion
- each tile reads the pbf again to find nodes within the tile bounds
- read way ids that connect to the tiles within the bounds for highways and areas
- read nodes, ways and relations based on the tile bounds

this is incredibly inefficient. the process needs to be changes so that the pbf file is read once and held in memory. create an in-memory pbf representation with a few additions:
- read all nodes and store them in memory 
- read all ways and store them in memory. attach info about it's bounding box by looking up nodes from the previous step and find min and max lat,lon
- read all relations and store them in memory. attach bounding box to each relation by looking up relation members (ways and nodes) from previous step. keep in mind that some relations may consist of other relations.

then process the tiles by doing the following:
- find tile min and max lat,lon from the in-memory pbf data
- calculate tiles
- process tiles in parallel
- for each tile look up pbf data from the in-nemory representation

the in-memory pbf representation needs to be designed with efficient lookups in mind. consider what is the most efficient data structure based on the access patterns, don't just use vectors. consider RTree from rstar or similar. Sacrifice memory useage to gain perf
