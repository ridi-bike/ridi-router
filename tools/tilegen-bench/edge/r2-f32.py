#!/usr/bin/env python3
"""f32 analysis of the global proximity grid and tile bounds (Rust f32 semantics emulated)."""
import math, struct

def f(x):
    return struct.unpack("f", struct.pack("f", x))[0]

C = f(0.0024); M = f(0.0012); TS = f(0.1); B = f(0.005)
lon_min = f(-180.0); lat_min = f(-90.0)
cols = math.ceil(f(f(f(180.0) - lon_min) / C)); rows = math.ceil(f(f(f(90.0) - lat_min) / C))
mcols = math.ceil(f(f(f(180.0) - lon_min) / M)); mrows = math.ceil(f(f(f(90.0) - lat_min) / M))
print(f"global grid: cols={cols} rows={rows} mcols={mcols} mrows={mrows}")

def col(lon, cell):  # ((lon as f32 - lon_min) / CELL) as i32
    return math.trunc(f(f(f(lon) - lon_min) / cell))

for lon in [180.0, 179.9999999, 179.9988, -180.0, -179.9999999]:
    c = col(lon, C); m = col(lon, M)
    print(f"lon {lon:13}: res col {c} in grid={0 <= c < cols}; mil col {m} in grid={0 <= m < mcols}")
for lat in [90.0, -90.0]:
    r = math.trunc(f(f(f(lat) - lat_min) / C)); print(f"lat {lat}: row {r} in grid={0 <= r < rows}")

# cell centre error and node->cell consistency near +-180 (f32 cell centre vs exact)
worst = 0.0
for c in list(range(0, 2000)) + list(range(cols - 2000, cols)):
    ctr = f(lon_min + f(f(c + 0.5) * C)) if False else f(lon_min + f(f(f(c) + f(0.5)) * C))
    worst = max(worst, abs(ctr - (-180 + (c + 0.5) * 0.0024)))
print(f"worst |f32 centre - exact centre| near +-180: {worst:.2e} deg (~{worst*111000:.2f} m at equator)")

# f32 resolution of node longitudes near 180: distinct decimicro values collapsing to one f32
ulp = f(180.0) - f(179.99999)  # rough
print(f"f32 spacing near 180: {math.ulp(f(179.9))*2**29:.2e}? (f32 has 24-bit mantissa) -> {2**(7-23):.2e} deg")

# node-on-cell-boundary: do decimicro coordinates exactly on a cell edge exist? cell edges are
# -180 + k*0.0024 = multiples of 0.0024 -> exact decimicro multiples of 24000.
# f32 rounding of lon can move such a node across the edge: count mismatches f32 vs exact
mis = 0; tot = 0
for k in range(0, cols, 97):
    edge_dec = -1800000000 + k * 24000
    for d in (-1, 0, 1):
        lon = (edge_dec + d) * 1e-7
        exact = (edge_dec + d + 1800000000) // 24000
        got = col(lon, C)
        tot += 1
        mis += got != exact
print(f"nodes within 1e-7 of a residential cell edge: {mis}/{tot} assigned to a different cell than exact maths (deterministic, same in all impls)")

# tile columns: max col, buffered bounds at the edges
ncols = math.ceil(360.0 / TS)  # f64 of f32 0.1
print(f"tile cols: 360/0.1f32 = {360.0/TS!r} -> ceil {ncols}; reference iterates col < ceil((min(lon_max+0.01,180)+180)/ts) = {math.ceil((180+180)/TS)} => max col {ncols-1}")
lo0 = max(f(f(f(0 * TS) - 180.0) - B), -180.0); hi_last = min(f(f(f(f((ncols - 1) * TS) - 180.0) + TS) + B), 180.0)
print(f"col 0 buffered lon_min={lo0}; col {ncols-1} buffered lon_max={hi_last} (no wraparound)")
lat_rows = math.ceil(180.0 / TS); print(f"tile rows: max row {lat_rows-1}")
