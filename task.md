I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: field `tile_id` is never read
  --> src/rmdf/generator/pbf_streamer.rs:55:5
   |
54 | struct TileData {
   |        -------- field in this struct
55 |     tile_id: TileId,
   |     ^^^^^^^
```
