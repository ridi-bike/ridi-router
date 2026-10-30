# Tile generator test fixtures

Small PBFs for `tests/goldens.rs`. All were cut with `osmium extract -s simple -b <bbox>`
(from Geofabrik extracts of October 2026) except `synthetic.osm.pbf`.

| File | Source / bbox | Covers |
|---|---|---|
| `fiji-east-180.osm.pbf` | Fiji, `179.8,-17.0,180,-16.6` | nodes at exactly lon +180, last tile column, +180 cell clamp |
| `fiji-west-180.osm.pbf` | Fiji, `-180,-17.5,-179,-16` | nodes at exactly lon -180, column 0, negative lon |
| `samoa-apia.osm.pbf` | Samoa, `-171.80,-13.86,-171.74,-13.82` | southern/western hemisphere, restrictions |
| `tonga-nukualofa.osm.pbf` | Tonga, `-175.23,-21.16,-175.17,-21.12` | southern/western hemisphere |
| `svalbard-78n.osm.pbf` | Svalbard, `15,78,16.6,78.4` | high latitude (500 m radius widening) |
| `riga-center.osm.pbf` | Latvia, `24.09,56.94,24.15,56.965` | dense city; tile sizes 0.05 / 0.1 / 0.25 |
| `border-lv.osm.pbf`, `border-lt.osm.pbf` | Latvia and Lithuania extracts, both `24.0,56.22,24.2,56.34` | directory input with overlapping files (~570 shared ways) |
| `synthetic.osm.pbf` | `synthetic.py` | multipolygon role quirks, nested area relations, military area, via node only in a tile buffer, nodes on cell edges |
