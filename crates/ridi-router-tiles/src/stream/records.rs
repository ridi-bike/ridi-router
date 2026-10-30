//! Record types and their binary encodings (spill files and region buckets).

use super::util::*;

/// Way tags (name, ref, highway, surface, smoothness, oneway, junction) in this fixed order.

pub type WayTags = [Option<Box<str>>; 7];

pub fn put_way_tags(buf: &mut Vec<u8>, tags: &[Option<&[u8]>; 7]) {
    let mut mask = 0u8;
    for (i, t) in tags.iter().enumerate() {
        if t.is_some() {
            mask |= 1 << i;
        }
    }
    buf.push(mask);
    for t in tags.iter().flatten() {
        put_str(buf, t);
    }
}

/// Copy encoded way tags (as produced by `put_way_tags`) from `src` at `pos` to `dst`.
pub fn copy_way_tags(src: &[u8], pos: &mut usize, dst: &mut Vec<u8>) {
    let start = *pos;
    let mask = src[*pos];
    *pos += 1;
    for i in 0..7 {
        if mask & (1 << i) != 0 {
            let _ = get_str(src, pos);
        }
    }
    dst.extend_from_slice(&src[start..*pos]);
}

pub fn get_way_tags(buf: &[u8], pos: &mut usize) -> WayTags {
    let mask = buf[*pos];
    *pos += 1;
    let mut tags: WayTags = Default::default();
    for (i, t) in tags.iter_mut().enumerate() {
        if mask & (1 << i) != 0 {
            let s = get_str(buf, pos);
            *t = Some(String::from_utf8_lossy(s).into_owned().into_boxed_str());
        }
    }
    tags
}

/// Restriction relation, reduced to what `GenerationGraph::insert_relation` reads.
#[derive(Clone, Debug)]
pub struct RestrictionRel {
    pub id: u64,
    pub tags: Vec<(String, String)>,
    /// (member type 0=node 1=way 2=relation, role 0=from 1=to 2=via 3=other, ref)
    pub members: Vec<(u8, u8, u64)>,
}

impl RestrictionRel {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        put_uvarint(buf, self.id);
        put_uvarint(buf, self.tags.len() as u64);
        for (k, v) in &self.tags {
            put_str(buf, k.as_bytes());
            put_str(buf, v.as_bytes());
        }
        put_uvarint(buf, self.members.len() as u64);
        for (t, r, id) in &self.members {
            buf.push(*t);
            buf.push(*r);
            put_uvarint(buf, *id);
        }
    }

    pub fn decode(buf: &[u8], pos: &mut usize) -> Self {
        let id = get_uvarint(buf, pos);
        let nt = get_uvarint(buf, pos) as usize;
        let mut tags = Vec::with_capacity(nt);
        for _ in 0..nt {
            let k = String::from_utf8_lossy(get_str(buf, pos)).into_owned();
            let v = String::from_utf8_lossy(get_str(buf, pos)).into_owned();
            tags.push((k, v));
        }
        let nm = get_uvarint(buf, pos) as usize;
        let mut members = Vec::with_capacity(nm);
        for _ in 0..nm {
            let t = buf[*pos];
            let r = buf[*pos + 1];
            *pos += 2;
            members.push((t, r, get_uvarint(buf, pos)));
        }
        Self { id, tags, members }
    }
}

pub const ROLE_OUTER: u8 = 0;
pub const ROLE_INNER: u8 = 1;
pub const ROLE_OTHER: u8 = 2;

/// Residential / military area relation.
#[derive(Clone, Debug)]
pub struct AreaRel {
    pub id: u64,
    pub residential: bool,
    pub military: bool,
    pub multipolygon: bool,
    /// Way members in order: (way id, role class).
    pub ways: Vec<(u64, u8)>,
}

/// Polygon kinds.
pub const POLY_RESIDENTIAL: u8 = 0;
pub const POLY_MILITARY: u8 = 1;

/// Ordering key reproducing the reference polygon order (closed ways by id, then relations).
pub type PolyKey = (u8, u64, u32);

/// Encoded polygon: one polygon (exterior + holes) in decimicro coordinates, with the
/// precomputed metadata the proximity maths needs.
pub struct PolyMeta {
    pub kind: u8,
    pub key: PolyKey,
    pub area: f32,
    pub centroid: (f64, f64),
    pub bbox: [f32; 4], // lat_min, lat_max, lon_min, lon_max
}

pub fn encode_polygon(buf: &mut Vec<u8>, meta: &PolyMeta, rings: &[Vec<(i32, i32)>]) {
    buf.push(meta.kind);
    buf.push(meta.key.0);
    put_uvarint(buf, meta.key.1);
    put_uvarint(buf, meta.key.2 as u64);
    put_f32(buf, meta.area);
    put_f64(buf, meta.centroid.0);
    put_f64(buf, meta.centroid.1);
    for v in meta.bbox {
        put_f32(buf, v);
    }
    put_uvarint(buf, rings.len() as u64);
    for ring in rings {
        put_uvarint(buf, ring.len() as u64);
        let (mut pla, mut plo) = (0i64, 0i64);
        for &(la, lo) in ring {
            put_svarint(buf, la as i64 - pla);
            put_svarint(buf, lo as i64 - plo);
            pla = la as i64;
            plo = lo as i64;
        }
    }
}

pub fn decode_polygon(buf: &[u8], pos: &mut usize) -> (PolyMeta, Vec<Vec<(i32, i32)>>) {
    let kind = buf[*pos];
    let k0 = buf[*pos + 1];
    *pos += 2;
    let k1 = get_uvarint(buf, pos);
    let k2 = get_uvarint(buf, pos) as u32;
    let area = get_f32(buf, pos);
    let c0 = get_f64(buf, pos);
    let c1 = get_f64(buf, pos);
    let mut bbox = [0f32; 4];
    for b in bbox.iter_mut() {
        *b = get_f32(buf, pos);
    }
    let nr = get_uvarint(buf, pos) as usize;
    let mut rings = Vec::with_capacity(nr);
    for _ in 0..nr {
        let n = get_uvarint(buf, pos) as usize;
        let mut ring = Vec::with_capacity(n);
        let (mut la, mut lo) = (0i64, 0i64);
        for _ in 0..n {
            la += get_svarint(buf, pos);
            lo += get_svarint(buf, pos);
            ring.push((la as i32, lo as i32));
        }
        rings.push(ring);
    }
    (
        PolyMeta {
            kind,
            key: (k0, k1, k2),
            area,
            centroid: (c0, c1),
            bbox,
        },
        rings,
    )
}
