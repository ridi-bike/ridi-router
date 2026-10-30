#!/usr/bin/env python3
"""Generate synthetic.osm: hand-made cases for the tile generator's regression tests.

Regenerate the PBF with:
    python3 synthetic.py > synthetic.osm && osmium sort synthetic.osm -o synthetic.osm.pbf --overwrite

Scenarios (each at its own place around 10 E / 50 N, tile size 0.1):
  1. residential multipolygon with an outer of empty role, an inner hole and a member of unknown
     role, plus a closed residential way; a road runs through both
  2. nested residential relations, one and two levels deep
  3. military closed way with a road through it
  4. turn restriction whose via node lies only in the buffer of the from/to ways' tile
  5. road nodes exactly on residential (0.0024) and military (0.0012) cell edges
"""

import sys

nodes = []  # (id, lat, lon)
ways = []  # (id, [node ids], {tags})
relations = []  # (id, [(type, ref, role)], {tags})
_next = {"n": 1, "w": 1, "r": 1}


def node(lat, lon):
    i = _next["n"]
    _next["n"] += 1
    nodes.append((i, lat, lon))
    return i


def way(node_ids, **tags):
    i = _next["w"]
    _next["w"] += 1
    ways.append((i, node_ids, tags))
    return i


def relation(members, **tags):
    i = _next["r"]
    _next["r"] += 1
    relations.append((i, members, tags))
    return i


def square(lat, lon, dlat, dlon, **tags):
    a = node(lat, lon)
    b = node(lat, lon + dlon)
    c = node(lat + dlat, lon + dlon)
    d = node(lat + dlat, lon)
    return way([a, b, c, d, a], **tags)


def road(points, **tags):
    tags.setdefault("highway", "residential")
    return way([node(lat, lon) for lat, lon in points], **tags)


def line(lat0, lon0, lat1, lon1, n):
    return [(lat0 + (lat1 - lat0) * k / n, lon0 + (lon1 - lon0) * k / n) for k in range(n + 1)]


# 1. multipolygon role quirks + closed way
outer = square(50.000, 10.000, 0.006, 0.006)
inner = square(50.002, 10.002, 0.001, 0.001)
odd = square(50.010, 10.000, 0.001, 0.001)
relation(
    [("way", outer, ""), ("way", inner, "inner"), ("way", odd, "foo")],
    type="multipolygon",
    landuse="residential",
)
square(50.000, 10.020, 0.006, 0.006, landuse="residential")
road(line(49.990, 10.003, 50.015, 10.003, 25), name="Quirk Street")
road(line(49.990, 10.023, 50.015, 10.023, 25), name="Closed Way Street")

# 2. nested relations: one level (R -> R) and two levels (R -> R -> R)
leaf1 = relation([("way", square(50.000, 10.040, 0.006, 0.006), "outer")], type="multipolygon", landuse="residential")
relation([("relation", leaf1, "")], type="multipolygon", landuse="residential")
leaf2 = relation([("way", square(50.020, 10.040, 0.006, 0.006), "outer")], type="multipolygon", landuse="residential")
mid2 = relation([("relation", leaf2, "")], type="multipolygon")
relation([("relation", mid2, "")], type="multipolygon", landuse="residential")
road(line(49.990, 10.043, 50.035, 10.043, 45), name="Nested Street")

# 3. military area
square(50.000, 10.060, 0.004, 0.004, landuse="military")
road(line(49.995, 10.062, 50.010, 10.062, 15), highway="tertiary", name="Base Road")

# 4. restriction with the via node in the buffer east of the tile edge at 10.1
via = node(50.050, 10.103)
from_way = way([node(50.050, 10.095), node(50.050, 10.099), via], highway="secondary", name="From")
to_way = way([via, node(50.047, 10.103), node(50.044, 10.103)], highway="secondary", name="To")
relation(
    [("way", from_way, "from"), ("node", via, "via"), ("way", to_way, "to")],
    type="restriction",
    restriction="no_right_turn",
)

# 5. nodes on cell edges of the global grids (-180/-90 origin)
road([(50.004, 10.0008), (50.0064, 10.0008), (50.0064, 10.0032), (50.0076, 10.0044)], name="Edge Lane")

out = sys.stdout
out.write('<?xml version="1.0" encoding="UTF-8"?>\n<osm version="0.6" generator="synthetic.py">\n')
for i, lat, lon in nodes:
    out.write(f'  <node id="{i}" version="1" lat="{lat:.7f}" lon="{lon:.7f}"/>\n')
for i, refs, tags in ways:
    out.write(f'  <way id="{i}" version="1">\n')
    for r in refs:
        out.write(f'    <nd ref="{r}"/>\n')
    for k, v in tags.items():
        out.write(f'    <tag k="{k}" v="{v}"/>\n')
    out.write("  </way>\n")
for i, members, tags in relations:
    out.write(f'  <relation id="{i}" version="1">\n')
    for t, ref, role in members:
        out.write(f'    <member type="{t}" ref="{ref}" role="{role}"/>\n')
    for k, v in tags.items():
        out.write(f'    <tag k="{k}" v="{v}"/>\n')
    out.write("  </relation>\n")
out.write("</osm>\n")
