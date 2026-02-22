I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: associated items `new`, `get_line_strings_from_boundary`, `match_holes_to_outer_polygons`, `read`, and `get_area_grid` are never used
   --> src/osm_data/pbf_area_reader.rs:32:12
    |
31  | impl<'a> PbfAreaReader<'a> {
    | -------------------------- associated items in this implementation
32  |     pub fn new(pbf: &'a mut OsmPbfReader<File>) -> Self {
    |            ^^^
...
41  |     fn get_line_strings_from_boundary(&self, boundary: &Boundary, role: &str) -> Vec<LineString> {
    |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
169 |     fn match_holes_to_outer_polygons(
    |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
194 |     pub fn read<T>(&mut self, selection: &T) -> Result<(), OsmDataReaderError>
    |            ^^^^
...
273 |     pub fn get_area_grid(self) -> AreaGrid {
    |            ^^^^^^^^^^^^^
```
