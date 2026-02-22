I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: methods `get_or_create`, `save_all`, `flush_to_disk`, and `flush_to_redb` are never used
   --> src/rmdf/generator/intermediate.rs:262:12
    |
255 | impl TileBuffers {
    | ---------------- methods in this implementation
...
262 |     pub fn get_or_create(&mut self, tile_id: TileId) -> &mut IntermediateTile {
    |            ^^^^^^^^^^^^^
...
267 |     pub fn save_all(&self, output_dir: &std::path::Path) -> anyhow::Result<()> {
    |            ^^^^^^^^
...
275 |     pub fn flush_to_disk(&mut self, output_dir: &std::path::Path) -> anyhow::Result<()> {
    |            ^^^^^^^^^^^^^
...
291 |     pub fn flush_to_redb(&mut self, db: &Database) -> anyhow::Result<()> {
    |            ^^^^^^^^^^^^^
```
