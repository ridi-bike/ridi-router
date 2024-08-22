use std::{cell::RefCell, fmt::Debug, slice::Iter};

use super::graph::{MapDataPointRef, MapDataWayRef};

#[derive(Clone)]
pub struct MapDataWay {
    pub id: u64,
    pub points: MapDataWayPoints,
}

// impl MapDataWay {
//     pub fn add_point(way: MapDataWayRef, point: MapDataPointRef) -> () {
//         let mut way_mut = way.borrow();
//         way_mut.points.add(point);
//     }
// }

impl PartialEq for MapDataWay {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Debug for MapDataWay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MapDataWay
    id={}
    points={:?}",
            self.id,
            self.points
                .iter()
                .map(|p| p.borrow().id)
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapDataWayPoints {
    points: Vec<MapDataPointRef>,
}
impl MapDataWayPoints {
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }
    pub fn from_vec(points: Vec<MapDataPointRef>) -> Self {
        Self { points }
    }

    pub fn is_first_or_last(&self, point: &MapDataPointRef) -> bool {
        let is_first = if let Some(ref first) = self.points.first() {
            first.borrow().id == point.borrow().id
        } else {
            false
        };
        let is_last = if let Some(ref last) = self.points.last() {
            last.borrow().id == point.borrow().id
        } else {
            false
        };

        is_first || is_last
    }

    pub fn get_after(&self, idx: usize) -> Option<&MapDataPointRef> {
        self.points.get(idx + 1)
    }

    pub fn get_before(&self, idx: usize) -> Option<&MapDataPointRef> {
        if idx == 0 {
            return None;
        }
        self.points.get(idx - 1)
    }

    pub fn iter(&self) -> Iter<'_, MapDataPointRef> {
        self.points.iter()
    }

    pub fn add(&mut self, point: MapDataPointRef) -> () {
        self.points.push(point);
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }
}

impl<'a> IntoIterator for &'a MapDataWayPoints {
    type Item = &'a MapDataPointRef;

    type IntoIter = std::slice::Iter<'a, MapDataPointRef>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.iter()
    }
}

impl<'a> IntoIterator for MapDataWayPoints {
    type Item = MapDataPointRef;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.into_iter()
    }
}
