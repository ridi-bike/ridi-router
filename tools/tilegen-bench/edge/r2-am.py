#!/usr/bin/env python3
"""Antimeridian / boundary statistics of routable ways. usage: r2-am.py <input.pbf> <workdir>"""
import subprocess, sys, os

inp, wd = sys.argv[1], sys.argv[2]
os.makedirs(wd, exist_ok=True)
F = "/home/toms/dev/ridi-router/map-data/bench/tmp/C-dev/r1/filter.txt"
ALLOWED = set("motorway trunk primary secondary tertiary unclassified residential motorway_link trunk_link primary_link secondary_link tertiary_link living_street track escape raceway road".split())
subprocess.run(["osmium", "tags-filter", "-O", "--no-progress", "-R", inp, "w/highway", "-o", f"{wd}/hw.pbf"], check=True)
# need nodes: re-run with referenced nodes then add locations
subprocess.run(["osmium", "tags-filter", "-O", "--no-progress", inp, "w/highway", "-o", f"{wd}/hwn.pbf"], check=True)
subprocess.run(["osmium", "add-locations-to-ways", "-O", "--no-progress", "--ignore-missing-nodes", f"{wd}/hwn.pbf", "-f", "opl", "-o", f"{wd}/hw.opl"], check=True)

near = at180 = crossing = routable = 0
cross_examples = []
at_examples = []
seen_nodes = set()
with open(f"{wd}/hw.opl") as f:
    for line in f:
        if not line.startswith("w"):
            continue
        parts = line.rstrip("\n").split(" ")
        wid = parts[0]
        tags = {}
        nodes = []
        for p in parts[1:]:
            if p.startswith("T") and len(p) > 1:
                for kv in p[1:].split(","):
                    if "=" in kv:
                        k, v = kv.split("=", 1); tags[k] = v
            elif p.startswith("N") and len(p) > 1:
                for n in p[1:].split(","):
                    nid, _, loc = n.partition("x")
                    if not loc:
                        nodes.append((nid, None)); continue
                    lon, _, lat = loc.partition("y")
                    nodes.append((nid, (float(lon), float(lat))))
        hw = tags.get("highway")
        if not (hw in ALLOWED or (hw == "path" and tags.get("motorcycle") == "yes")):
            continue
        routable += 1
        for nid, loc in nodes:
            if loc and nid not in seen_nodes:
                seen_nodes.add(nid)
                if abs(loc[0]) >= 179.995:
                    near += 1
                if abs(loc[0]) == 180.0:
                    at180 += 1
                    if len(at_examples) < 5: at_examples.append((wid, nid, loc))
        for (a, la), (b, lb) in zip(nodes, nodes[1:]):
            if la and lb and abs(la[0] - lb[0]) > 180:
                crossing += 1
                if len(cross_examples) < 10: cross_examples.append((wid, a, la, b, lb))
print(f"routable ways={routable} routable nodes={len(seen_nodes)} nodes |lon|>=179.995: {near}  exactly ±180: {at180}  segments crossing 180: {crossing}")
print("at180 examples:", at_examples)
print("crossing examples:", cross_examples)
