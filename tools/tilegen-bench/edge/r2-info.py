#!/usr/bin/env python3
import os, subprocess, sys
D = "/home/toms/dev/ridi-router/map-data/input-world"
for name in sys.argv[1:]:
    p = f"{D}/{name}-latest.osm.pbf"
    if not os.path.exists(p):
        print(f"{name:28s} MISSING"); continue
    hb = subprocess.run(["osmium", "fileinfo", "-g", "header.boxes", p], capture_output=True, text=True).stdout.strip().replace("\n", " ")
    print(f"{name:28s} {os.path.getsize(p)/1e6:8.1f} MB  {hb}")
