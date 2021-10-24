use std::convert::TryInto;
use std::hash::BuildHasher;
use std::hash::Hash;
use std::hash::Hasher;

use nohash::IntMap;
use parking_lot::MappedRwLockReadGuard;
use parking_lot::MappedRwLockWriteGuard;
use parking_lot::RwLock;
use parking_lot::RwLockReadGuard;
use parking_lot::RwLockWriteGuard;

use crate::data::GameState;

pub struct StateMap<V, S = ahash::RandomState> {
    hasher: S,
    buckets: Box<[RwLock<IntMap<u64, V>>; SHARDS]>,
}

const SHARD_INDEX_BITS: usize = 12;
const SHARD_INDEX_SHIFT: usize = 32;
const SHARDS: usize = 1 << SHARD_INDEX_BITS;

impl<V, S: Default> Default for StateMap<V, S> {
    fn default() -> Self {
        StateMap {
            hasher: Default::default(),
            buckets: std::iter::repeat_with(|| RwLock::new(IntMap::default()))
                .take(SHARDS)
                .collect::<Box<_>>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}

impl<V, S: BuildHasher> StateMap<V, S> {
    pub fn index(&self, k: &GameState) -> u64 {
        let mut hasher = self.hasher.build_hasher();
        k.hash(&mut hasher);
        hasher.finish()
    }

    fn bucket(&self, k: u64) -> &RwLock<IntMap<u64, V>> {
        &self.buckets[(k >> SHARD_INDEX_SHIFT) as usize % SHARDS]
    }

    pub fn get_raw(&self, k: u64) -> Option<MappedRwLockReadGuard<V>> {
        RwLockReadGuard::try_map(self.bucket(k).read(), |shard| shard.get(&k)).ok()
    }

    pub fn get(&self, k: &GameState) -> Option<MappedRwLockReadGuard<V>> {
        self.get_raw(self.index(k))
    }

    pub fn get_raw_mut(&self, k: u64) -> Option<MappedRwLockWriteGuard<V>> {
        RwLockWriteGuard::try_map(self.bucket(k).write(), |shard| shard.get_mut(&k)).ok()
    }

    pub fn get_raw_or_insert_with(
        &self,
        k: u64,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<V> {
        RwLockWriteGuard::map(self.bucket(k).write(), |shard| {
            shard.entry(k).or_insert_with(f)
        })
    }

    pub fn get_or_insert_with(
        &self,
        k: &GameState,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<V> {
        self.get_raw_or_insert_with(self.index(k), f)
    }
    pub fn map_values<T>(self, f: impl Fn(V) -> T) -> StateMap<T, S> {
        StateMap {
            hasher: self.hasher,
            buckets: self
                .buckets
                .into_iter()
                .map(|shard| {
                    RwLock::new(
                        shard
                            .into_inner()
                            .into_iter()
                            .map(|(k, v)| (k, f(v)))
                            .collect(),
                    )
                })
                .collect::<Box<_>>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}
