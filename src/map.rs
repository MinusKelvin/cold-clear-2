use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::BuildHasher;
use std::hash::Hash;
use std::hash::Hasher;

use parking_lot::MappedRwLockReadGuard;
use parking_lot::MappedRwLockWriteGuard;
use parking_lot::RwLock;
use parking_lot::RwLockReadGuard;
use parking_lot::RwLockWriteGuard;

use crate::profile::ProfileScope;

pub struct Map<K, V, S = RandomState> {
    hasher: S,
    buckets: Box<[RwLock<HashMap<K, V, S>>; SHARDS]>,
}

const SHARDS: usize = 4096;

impl<K, V, S: Default> Default for Map<K, V, S> {
    fn default() -> Self {
        Map {
            hasher: Default::default(),
            buckets: std::iter::repeat_with(|| RwLock::new(HashMap::default()))
                .take(SHARDS)
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> Map<K, V, S> {
    fn bucket(&self, k: &K) -> &RwLock<HashMap<K, V, S>> {
        let mut hasher = self.hasher.build_hasher();
        k.hash(&mut hasher);
        let i = hasher.finish() as usize % SHARDS;
        &self.buckets[i]
    }

    pub fn get(&self, k: &K) -> Option<MappedRwLockReadGuard<V>> {
        RwLockReadGuard::try_map(profile_read(self.bucket(k)), |shard| shard.get(k)).ok()
    }

    pub fn get_mut(&self, k: &K) -> Option<MappedRwLockWriteGuard<V>> {
        RwLockWriteGuard::try_map(profile_write(self.bucket(k)), |shard| shard.get_mut(k)).ok()
    }

    pub fn insert(&self, k: K, v: V) -> Option<V> {
        profile_write(self.bucket(&k)).insert(k, v)
    }

    pub fn get_or_insert_with(&self, k: K, f: impl FnOnce() -> V) -> MappedRwLockWriteGuard<V> {
        RwLockWriteGuard::map(profile_write(self.bucket(&k)), |shard| {
            shard.entry(k).or_insert_with(f)
        })
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
