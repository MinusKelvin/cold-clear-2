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
use crate::profile::ProfileScope;

pub struct StateMap<V, S = ahash::RandomState> {
    hasher: S,
    buckets: Box<[RwLock<IntMap<u64, V>>; SHARDS]>,
}

const SHARD_INDEX_BITS: usize = 12;
const SHARD_INDEX_SHIFT: usize = 64 - 12;
const SHARDS: usize = 1 << SHARD_INDEX_BITS;

impl<V, S: Default> Default for StateMap<V, S> {
    fn default() -> Self {
        StateMap {
            hasher: Default::default(),
            buckets: std::iter::repeat_with(|| RwLock::new(IntMap::default()))
                .take(SHARDS)
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}

impl<V, S: BuildHasher> StateMap<V, S> {
    pub fn index(&self, k: &GameState) -> u64 {
        let _scope = ProfileScope::new("hashing");
        let mut hasher = self.hasher.build_hasher();
        k.hash(&mut hasher);
        hasher.finish()
    }

    fn bucket(&self, k: u64) -> &RwLock<IntMap<u64, V>> {
        &self.buckets[(k >> SHARD_INDEX_SHIFT) as usize]
    }

    pub fn get_raw(&self, k: u64) -> Option<MappedRwLockReadGuard<V>> {
        RwLockReadGuard::try_map(profile_read(self.bucket(k)), |shard| shard.get(&k)).ok()
    }

    pub fn get(&self, k: &GameState) -> Option<MappedRwLockReadGuard<V>> {
        self.get_raw(self.index(k))
    }

    pub fn get_raw_mut(&self, k: u64) -> Option<MappedRwLockWriteGuard<V>> {
        RwLockWriteGuard::try_map(profile_write(self.bucket(k)), |shard| shard.get_mut(&k)).ok()
    }

    pub fn get_mut(&self, k: &GameState) -> Option<MappedRwLockWriteGuard<V>> {
        self.get_raw_mut(self.index(k))
    }

    pub fn insert(&self, k: &GameState, v: V) {
        let i = self.index(k);
        profile_write(self.bucket(i)).insert(i, v);
    }

    pub fn get_raw_or_insert_with(
        &self,
        k: u64,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<V> {
        RwLockWriteGuard::map(profile_write(self.bucket(k)), |shard| {
            if !shard.contains_key(&k) {
                shard.insert(k, f());
            }
            shard.get_mut(&k).unwrap()
        })
    }

    pub fn get_or_insert_with(
        &self,
        k: &GameState,
        f: impl FnOnce() -> V,
    ) -> MappedRwLockWriteGuard<V> {
        self.get_raw_or_insert_with(self.index(k), f)
    }
}

fn profile_read<T>(lock: &RwLock<T>) -> RwLockReadGuard<T> {
    if let Some(guard) = lock.try_read() {
        return guard;
    }
    let _scope = ProfileScope::new("map read contention");
    lock.read()
}

fn profile_write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<T> {
    if let Some(guard) = lock.try_write() {
        return guard;
    }
    let _scope = ProfileScope::new("map write contention");
    lock.write()
}
