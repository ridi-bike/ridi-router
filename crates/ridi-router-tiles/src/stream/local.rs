//! Tile-stage job: load the region buckets of one job, convert them to the shared
//! `local_stage` input (prototype A's module, vendored unchanged) and run it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use geo::{Coord, LineString, MultiPolygon, Polygon};
use ridi_router_common::manifest::TileMetadata;
use ridi_router_common::osm::{
    OsmRelation, OsmRelationMember, OsmRelationMemberRole, OsmRelationMemberType,
};

use super::buckets::{REC_POLY, REC_RESTRICTION, REC_WAY};
use super::records::*;
use super::util::*;
use crate::local_stage::{self, GridAnchor, LocalStageInput, NodeTable, WayTable, MISSING_COORD};

/// Margin around a job's tile rectangle when selecting shared large polygons (covers the 500 m
/// prefilter, tile buffer and nodes of ways that leave the rectangle).
const JOB_POLY_MARGIN_DEG: f64 = 0.05;

#[derive(Default, Debug, Clone, Copy)]
pub struct RegionStats {
    pub tiles: u64,
    pub nodes: u64,
    pub ways: u64,
    pub polygons: u64,
}

impl RegionStats {
    pub fn add(mut self, o: Self) -> Self {
        self.tiles += o.tiles;
        self.nodes += o.nodes;
        self.ways += o.ways;
        self.polygons += o.polygons;
        self
    }
}

/// Wall time summed over jobs per local-stage sub-phase (for the tiles PHASE breakdown).
#[derive(Default)]
pub struct SubPhaseTimes {
    pub parse_ns: AtomicU64,
    pub tile_assignment_ns: AtomicU64,
    pub proximity_ns: AtomicU64,
    pub write_tiles_ns: AtomicU64,
}

impl SubPhaseTimes {
    pub fn summary(&self) -> String {
        let s = |a: &AtomicU64| a.load(Ordering::Relaxed) as f64 / 1e9;
        format!(
            "job-summed wall: parse {:.1}s, tile_assignment {:.1}s, proximity {:.1}s, write_tiles {:.1}s",
            s(&self.parse_ns),
            s(&self.tile_assignment_ns),
            s(&self.proximity_ns),
            s(&self.write_tiles_ns)
        )
    }
}

pub struct PolyEntry {
    kind: u8,
    key: PolyKey,
    /// lat_min, lat_max, lon_min, lon_max (f32 exterior bbox)
    bbox: [f32; 4],
    mp: MultiPolygon<f64>,
}

fn build_mp(rings: Vec<Vec<(i32, i32)>>) -> MultiPolygon<f64> {
    let mut ls = rings.into_iter().map(|r| {
        LineString::new(
            r.into_iter()
                .map(|(la, lo)| Coord {
                    x: decimicro_to_deg(lo),
                    y: decimicro_to_deg(la),
                })
                .collect(),
        )
    });
    let ext = ls.next().unwrap();
    let holes: Vec<_> = ls.collect();
    MultiPolygon::new(vec![Polygon::new(ext, holes)])
}

/// Polygons touching many regions, kept once and handed to every job they are near.
pub struct GlobalPolys {
    polys: Vec<PolyEntry>,
}

impl GlobalPolys {
    pub fn load(path: &Path) -> Result<Self> {
        let mut polys = Vec::new();
        if path.exists() {
            let data = std::fs::read(path)?;
            let mut pos = 0;
            while pos < data.len() {
                let len = get_uvarint(&data, &mut pos) as usize;
                let mut p = 0;
                let (meta, rings) = decode_polygon(&data[pos..pos + len], &mut p);
                pos += len;
                polys.push(PolyEntry {
                    kind: meta.kind,
                    key: meta.key,
                    bbox: meta.bbox,
                    mp: build_mp(rings),
                });
            }
        }
        Ok(Self { polys })
    }

    pub fn len(&self) -> usize {
        self.polys.len()
    }
}

/// A unit of tile-stage work: a rectangle of tiles and the bucket files that cover it.
pub struct Job {
    pub col0: u32,
    pub row0: u32,
    pub ncols: u32,
    pub nrows: u32,
    pub files: Vec<PathBuf>,
    pub bytes: u64,
}

struct LocalWay {
    id: u64,
    tags: WayTags,
    ids: Vec<u64>,
}

#[hotpath::measure]
pub fn process_job(
    job: &Job,
    tile_size: f32,
    anchor: GridAnchor,
    out_dir: &Path,
    global: &GlobalPolys,
    times: &SubPhaseTimes,
) -> Result<(RegionStats, Vec<TileMetadata>)> {
    let t0 = std::time::Instant::now();
    let mut ways: Vec<LocalWay> = Vec::new();
    let mut coords: IdMap<(i32, i32)> = IdMap::default();
    let mut polys: Vec<PolyEntry> = Vec::new();
    let mut restrictions: Vec<RestrictionRel> = Vec::new();
    for path in &job.files {
        let data = std::fs::read(path).with_context(|| format!("read bucket {path:?}"))?;
        parse_bucket(&data, path, &mut ways, &mut coords, &mut polys, &mut restrictions)?;
    }
    // records of objects spanning several base regions appear once per region
    ways.sort_unstable_by_key(|w| w.id);
    ways.dedup_by_key(|w| w.id);
    restrictions.sort_unstable_by_key(|r| r.id);
    restrictions.dedup_by_key(|r| r.id);

    // shared large polygons near this job
    let ts = tile_size as f64;
    let lon0 = job.col0 as f64 * ts - 180.0;
    let lat0 = job.row0 as f64 * ts - 90.0;
    let lon1 = (job.col0 + job.ncols) as f64 * ts - 180.0;
    let lat1 = (job.row0 + job.nrows) as f64 * ts - 90.0;
    let m = JOB_POLY_MARGIN_DEG;
    for p in &global.polys {
        let [la0, la1, lo0, lo1] = p.bbox;
        if (lo1 as f64) >= lon0 - m
            && (lo0 as f64) <= lon1 + m
            && (la1 as f64) >= lat0 - m
            && (la0 as f64) <= lat1 + m
        {
            polys.push(PolyEntry {
                kind: p.kind,
                key: p.key,
                bbox: p.bbox,
                mp: p.mp.clone(),
            });
        }
    }
    polys.sort_unstable_by_key(|p| (p.kind, p.key));
    polys.dedup_by_key(|p| (p.kind, p.key));
    let mut residential = Vec::new();
    let mut military = Vec::new();
    for p in polys {
        if p.kind == POLY_RESIDENTIAL {
            residential.push(p.mp);
        } else {
            military.push(p.mp);
        }
    }

    // node table: every referenced node, ascending id
    let mut node_ids: Vec<u64> = ways.iter().flat_map(|w| w.ids.iter().copied()).collect();
    node_ids.sort_unstable();
    node_ids.dedup();
    let mut lat_e7 = Vec::with_capacity(node_ids.len());
    let mut lon_e7 = Vec::with_capacity(node_ids.len());
    for id in &node_ids {
        match coords.get(id) {
            Some(&(la, lo)) => {
                lat_e7.push(la);
                lon_e7.push(lo);
            }
            None => {
                lat_e7.push(MISSING_COORD);
                lon_e7.push(MISSING_COORD);
            }
        }
    }
    drop(coords);

    // way table
    let mut way_ids = Vec::with_capacity(ways.len());
    let mut offsets = Vec::with_capacity(ways.len() + 1);
    let mut refs: Vec<u32> = Vec::new();
    let mut tag_set = Vec::with_capacity(ways.len());
    let mut tag_sets = Vec::with_capacity(ways.len());
    offsets.push(0usize);
    for (i, w) in ways.into_iter().enumerate() {
        way_ids.push(w.id);
        refs.extend(w.ids.iter().map(|id| {
            node_ids
                .binary_search(id)
                .map(|i| i as u32)
                .unwrap_or(u32::MAX)
        }));
        offsets.push(refs.len());
        tag_set.push(i as u32);
        let [name, hw_ref, highway, surface, smoothness, oneway, junction] =
            w.tags.map(|t| t.map(String::from));
        tag_sets.push(local_stage::WayTags {
            name,
            hw_ref,
            highway,
            surface,
            smoothness,
            oneway,
            junction,
        });
    }
    let relations: Vec<OsmRelation> = restrictions.iter().map(to_osm_relation).collect();
    times
        .parse_ns
        .fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let stats = RegionStats {
        tiles: 0,
        nodes: node_ids.len() as u64,
        ways: way_ids.len() as u64,
        polygons: (residential.len() + military.len()) as u64,
    };
    if way_ids.is_empty() {
        return Ok((stats, Vec::new()));
    }

    let input = LocalStageInput {
        output_dir: out_dir,
        tile_size_degrees: tile_size,
        anchor,
        core_bbox: Some([lon0, lat0, lon1, lat1]),
        nodes: NodeTable {
            ids: &node_ids,
            lat_e7: &lat_e7,
            lon_e7: &lon_e7,
        },
        ways: WayTable {
            ids: &way_ids,
            offsets: &offsets,
            refs: &refs,
            tag_set: &tag_set,
            tag_sets: &tag_sets,
        },
        restrictions: &relations,
        residential_polygons: &residential,
        military_polygons: &military,
    };
    let mut last = std::time::Instant::now();
    let out = local_stage::run(&input, &mut |name| {
        let ns = last.elapsed().as_nanos() as u64;
        last = std::time::Instant::now();
        match name {
            "tile_assignment" => &times.tile_assignment_ns,
            "proximity" => &times.proximity_ns,
            _ => &times.write_tiles_ns,
        }
        .fetch_add(ns, Ordering::Relaxed);
    })?;
    Ok((
        RegionStats {
            tiles: out.tiles.len() as u64,
            ..stats
        },
        out.tiles,
    ))
}

fn to_osm_relation(r: &RestrictionRel) -> OsmRelation {
    OsmRelation {
        id: r.id,
        tags: r.tags.iter().cloned().collect(),
        members: r
            .members
            .iter()
            .map(|&(t, role, id)| OsmRelationMember {
                member_type: match t {
                    0 => OsmRelationMemberType::Node,
                    1 => OsmRelationMemberType::Way,
                    _ => OsmRelationMemberType::Relation,
                },
                role: match role {
                    0 => OsmRelationMemberRole::From,
                    1 => OsmRelationMemberRole::To,
                    2 => OsmRelationMemberRole::Via,
                    _ => OsmRelationMemberRole::Other(String::new()),
                },
                member_ref: id,
            })
            .collect(),
    }
}

fn parse_bucket(
    data: &[u8],
    path: &Path,
    ways: &mut Vec<LocalWay>,
    coords: &mut IdMap<(i32, i32)>,
    polys: &mut Vec<PolyEntry>,
    restrictions: &mut Vec<RestrictionRel>,
) -> Result<()> {
    let mut pos = 0;
    while pos < data.len() {
        let kind = data[pos];
        pos += 1;
        let len = get_uvarint(data, &mut pos) as usize;
        let end = pos + len;
        anyhow::ensure!(end <= data.len(), "corrupt bucket {path:?}: truncated record");
        let rec = &data[pos..end];
        let mut p = 0;
        match kind {
            REC_WAY => {
                let id = get_uvarint(rec, &mut p);
                let tags = get_way_tags(rec, &mut p);
                let n = get_uvarint(rec, &mut p) as usize;
                let mut ids = Vec::with_capacity(n);
                let (mut nid, mut la, mut lo) = (0i64, 0i64, 0i64);
                for _ in 0..n {
                    let v = get_uvarint(rec, &mut p);
                    let zz = v >> 1;
                    nid += ((zz >> 1) as i64) ^ -((zz & 1) as i64);
                    ids.push(nid as u64);
                    if v & 1 == 1 {
                        la += get_svarint(rec, &mut p);
                        lo += get_svarint(rec, &mut p);
                        coords.entry(nid as u64).or_insert((la as i32, lo as i32));
                    }
                }
                ways.push(LocalWay { id, tags, ids });
            }
            REC_POLY => {
                let (meta, rings) = decode_polygon(rec, &mut p);
                polys.push(PolyEntry {
                    kind: meta.kind,
                    key: meta.key,
                    bbox: meta.bbox,
                    mp: build_mp(rings),
                });
            }
            REC_RESTRICTION => restrictions.push(RestrictionRel::decode(rec, &mut p)),
            _ => anyhow::bail!("corrupt bucket {path:?}: record kind {kind}"),
        }
        pos = end;
    }
    Ok(())
}
