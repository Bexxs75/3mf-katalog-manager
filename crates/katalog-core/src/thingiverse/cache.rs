//! Short-lived memory cache. Nothing here ever reaches the disk or the log: hits,
//! detail texts and preview images live for ten minutes at most and are dropped on
//! switching the area off, removing the key and closing the program.

use std::collections::HashMap;

pub const TTL_S: f64 = 600.0;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Search { query: String, page: u32 },
    Thing(u64),
    /// Addresses of images and page of a thing seen in a result.
    Refs(u64),
    Image { id: u64, large: bool },
}

struct Entry<V> {
    at: f64,
    bytes: usize,
    value: V,
}

pub struct Cache<V> {
    entries: HashMap<Key, Entry<V>>,
    total: usize,
}

impl<V: Clone> Default for Cache<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> Cache<V> {
    pub fn new() -> Self {
        Cache { entries: HashMap::new(), total: 0 }
    }

    fn expire(&mut self, now: f64) {
        let stale: Vec<Key> = self.entries.iter().filter(|(_, e)| now - e.at >= TTL_S).map(|(k, _)| k.clone()).collect();
        for k in stale {
            self.remove(&k);
        }
    }

    fn remove(&mut self, key: &Key) {
        if let Some(e) = self.entries.remove(key) {
            self.total -= e.bytes;
        }
    }

    pub fn get(&mut self, key: &Key, now: f64) -> Option<V> {
        self.expire(now);
        self.entries.get(key).map(|e| e.value.clone())
    }

    /// Stores a value of about `bytes` bytes. An entry larger than the whole budget is
    /// not stored; otherwise the oldest entries make room.
    pub fn put(&mut self, key: Key, value: V, bytes: usize, now: f64) {
        self.expire(now);
        self.remove(&key);
        if bytes > MAX_BYTES {
            return;
        }
        while self.total + bytes > MAX_BYTES {
            let oldest = self.entries.iter().min_by(|a, b| a.1.at.total_cmp(&b.1.at)).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => self.remove(&k),
                None => break,
            }
        }
        self.total += bytes;
        self.entries.insert(key, Entry { at: now, bytes, value });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.total = 0;
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn total_bytes(&self) -> usize {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(id: u64) -> Key {
        Key::Thing(id)
    }

    #[test]
    fn entries_expire_after_ten_minutes() {
        let mut c = Cache::<u8>::new();
        c.put(k(1), 7, 10, 0.0);
        assert_eq!(c.get(&k(1), 599.0), Some(7));
        assert_eq!(c.get(&k(1), 600.0), None);
        assert_eq!(c.total_bytes(), 0, "an expired entry also frees its bytes");
    }

    #[test]
    fn clear_empties_everything() {
        let mut c = Cache::<u8>::new();
        c.put(k(1), 1, 10, 0.0);
        c.put(k(2), 2, 10, 0.0);
        c.clear();
        assert_eq!((c.len(), c.total_bytes()), (0, 0));
        assert_eq!(c.get(&k(1), 1.0), None);
    }

    #[test]
    fn the_total_never_exceeds_eight_mib_and_the_oldest_go_first() {
        let mut c = Cache::<u32>::new();
        let third = MAX_BYTES / 3;
        for i in 0..10u64 {
            c.put(k(i), i as u32, third, i as f64);
            assert!(c.total_bytes() <= MAX_BYTES, "after {i}");
        }
        assert_eq!(c.get(&k(0), 10.0), None, "the oldest was evicted");
        assert_eq!(c.get(&k(9), 10.0), Some(9));
        c.put(k(99), 1, MAX_BYTES + 1, 11.0);
        assert_eq!(c.get(&k(99), 11.0), None, "an entry above the budget is not kept");
    }

    #[test]
    fn one_hundred_preview_payloads_stay_within_the_cache_budget() {
        let mut cache = Cache::<Vec<u8>>::new();
        for id in 0..100 {
            let preview = vec![0u8; 128 * 1024];
            cache.put(k(id), preview, 128 * 1024, id as f64);
            assert!(cache.total_bytes() <= MAX_BYTES);
        }
        assert!(cache.get(&k(0), 100.0).is_none());
        assert_eq!(cache.get(&k(99), 100.0).unwrap().len(), 128 * 1024);
        cache.clear();
        assert_eq!(cache.total_bytes(), 0);
        assert!(cache.get(&k(99), 100.0).is_none());
    }

    #[test]
    fn replacing_a_key_does_not_count_twice() {
        let mut c = Cache::<u8>::new();
        c.put(k(1), 1, 100, 0.0);
        c.put(k(1), 2, 100, 1.0);
        assert_eq!(c.total_bytes(), 100);
        assert_eq!(c.get(&k(1), 2.0), Some(2));
    }
}
