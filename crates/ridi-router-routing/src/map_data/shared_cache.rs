use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash},
    sync::Mutex,
};

use rustc_hash::FxBuildHasher;

const SHARDS: usize = 64;

/// Hydrated graph elements shared by every itinerary of a generation run, so parallel
/// itineraries exploring the same area hydrate each point once instead of once per itinerary.
pub(crate) struct SharedCache<K, V> {
    shards: Box<[Mutex<HashMap<K, V, FxBuildHasher>>]>,
}

impl<K: Hash + Eq + Clone, V: Clone> SharedCache<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            shards: (0..SHARDS).map(|_| Mutex::default()).collect(),
        }
    }

    fn shard(&self, key: &K) -> &Mutex<HashMap<K, V, FxBuildHasher>> {
        &self.shards[FxBuildHasher.hash_one(key) as usize % SHARDS]
    }

    /// `compute` runs without the shard lock held; it may take other locks.
    pub(crate) fn get_or_insert_with(&self, key: &K, compute: impl FnOnce() -> V) -> V {
        if let Some(value) = self.shard(key).lock().unwrap().get(key) {
            return value.clone();
        }
        let value = compute();
        self.shard(key)
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_insert(value)
            .clone()
    }

    pub(crate) fn clear(&self) {
        for shard in self.shards.iter() {
            *shard.lock().unwrap() = HashMap::default();
        }
    }
}
