//! Small bounded LRU map (no external deps).

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Insertion-order LRU: evicts oldest key when over capacity.
pub struct LruMap<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    cap: usize,
}

impl<K: Hash + Eq + Clone, V> LruMap<K, V> {
    /// Create an LRU with a maximum of `cap` entries (`cap` is at least 1).
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            map: HashMap::with_capacity(cap),
            order: VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Borrow a value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    /// Insert `key`/`value`, evicting the oldest entry if over capacity.
    ///
    /// Updating an existing key does not change eviction order (simple policy).
    pub fn insert(&mut self, key: K, value: V) {
        use std::collections::hash_map::Entry;
        if let Entry::Occupied(mut e) = self.map.entry(key.clone()) {
            e.insert(value);
            return;
        }
        while self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Drop all entries.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}
