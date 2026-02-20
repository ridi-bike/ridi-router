currently the `generate-tiles` command can only process a single pbf file. 
this is done by reading the entire file, processing all nodes, ways and relations to assign boudning boxes, compute nogo and proximity flags using rasterized residential and military area grids and simd for haversine distance. then tiles are calculated and all the data from the in-memory pbf is written to tiles.

this works nicely and is very fast but limited. we need to make a change to allow multiple pfb files to be read from a directory of pbf files. each file would be processed sequentially and the resulting tiles must be combined where they overlapp (and nodes, ways, relation deduped) so we can set a directory of the whole globe consisting of small pbf files and eventually we will get a list of tiles that cover the whole globe, don't have missing data and don't have duplicate data

please examine the codebase and give me several approaches for sticking the data together from multiple pbf files
