I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: fields `residential_ways`, `residential_relations`, `military_ways`, and `military_relations` are never read
   --> src/osm_data/in_memory_pbf.rs:197:5
    |
185 | pub struct InMemoryPbf {
    |            ----------- fields in this struct
...
197 |     residential_ways: Vec<u64>,
    |     ^^^^^^^^^^^^^^^^
198 |     residential_relations: Vec<u64>,
    |     ^^^^^^^^^^^^^^^^^^^^^
199 |     military_ways: Vec<u64>,
    |     ^^^^^^^^^^^^^
200 |     military_relations: Vec<u64>,
    |     ^^^^^^^^^^^^^^^^^^
```
