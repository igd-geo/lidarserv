use std::collections::{HashMap, hash_map};
use std::hash::Hash;
use std::{mem, ptr};

/// A map with last recently used ordering
pub struct Lru<K, V> {
    pub entries: HashMap<K, Entry<K, V>>,
    first: Option<K>,
    last: Option<K>,
}

pub struct Entry<K, V> {
    pub data: V,
    next: Option<K>,
    prev: Option<K>,
}

impl<K, V> Default for Lru<K, V> {
    fn default() -> Self {
        Lru {
            entries: HashMap::new(),
            first: None,
            last: None,
        }
    }
}

impl<K, V> Lru<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of stored elements
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl<K, V> Lru<K, V>
where
    K: Hash + Eq + Clone,
{
    /// Adds a new element to the linked list of entries.
    /// The element will be inserted right before the element with the key given
    /// by the insert_before parameter, or - if insert_before is None - at the end of the list.
    /// Safety / Invariants:
    ///  - The passed in pointer must either point to an entry in self.entries,
    ///     or the pointed-to entry must be inserted into self.entries immediately
    ///     after this operation.
    ///     Otherwise, the linked list will be left with invalid next/prev references.
    ///  - The inserted entry must be "dangling".
    ///     This means, that it is not already in the list.
    ///     More concretely, the fields entry.next and entry.prev both must
    ///     point to the entry itself.
    ///  - The insert_before key refer to an entry, that exist in self.entries.
    unsafe fn raw_list_insert(&mut self, ptr_entry: *mut Entry<K, V>, insert_before: Option<&K>) {
        unsafe {
            let Self {
                entries,
                first,
                last,
            } = self;

            // need to use pointers / unsafe, because we hold multiple mutable references
            // into the entries hashmap.
            // This is safe though, because we do not add/remove entries, so the pointers stay valid.

            // get pointers to all relevant prev/next fields in the linked list
            let ptr_entry_next = &mut (*ptr_entry).next as *mut Option<K>;
            let ptr_entry_prev = &mut (*ptr_entry).prev as *mut Option<K>;
            let ptr_next_prev = match insert_before {
                None => last as *mut Option<K>,
                Some(k) => &mut entries.get_mut(k).unwrap().prev as *mut Option<K>,
            };
            let ptr_prev_next = match &*ptr_next_prev {
                None => first as *mut Option<K>,
                Some(k) => &mut entries.get_mut(k).unwrap().next as *mut Option<K>,
            };

            // insert into list
            ptr::swap(ptr_entry_next, ptr_prev_next);
            ptr::swap(ptr_entry_prev, ptr_next_prev);
        }
    }

    /// Removes an element from the linked list of entries.
    /// Safety / Invariants:
    ///  - The removed entry will be left "dangling".
    ///    This means, that it is not in the list any more.
    ///    More concretely, the fields entry.next and entry.prev will
    ///    both point to the entry itself.
    ///  - The pointed-to entry does not need to be stored in self.entries. It is Ok, to first
    ///    move the entry out of self.entries, and then call this function to fix the linked list.
    ///    However, if it *is* stored in self.entries, then it either needs
    ///    to be removed afterwards, or re-inserted into the linked list using raw_list_insert.
    ///  - The next / previous keys of the pointed-to entry need to refer
    ///    to existing items in self.entries.
    unsafe fn raw_list_remove(&mut self, ptr_entry: *mut Entry<K, V>) {
        unsafe {
            // need to use pointers / unsafe, because we hold multiple mutable references
            // into the entries hashmap.
            // This is safe though, because we do not
            // add/remove entries, so the pointers stay valid.

            let ptr_entry_next = &mut (*ptr_entry).next as *mut Option<K>;
            let ptr_entry_prev = &mut (*ptr_entry).prev as *mut Option<K>;
            let ptr_next_prev = match &*ptr_entry_next {
                None => &mut self.last as *mut Option<K>,
                Some(k) => &mut self.entries.get_mut(k).unwrap().prev as *mut Option<K>,
            };
            let ptr_prev_next = match &*ptr_entry_prev {
                None => &mut self.first as *mut Option<K>,
                Some(k) => &mut self.entries.get_mut(k).unwrap().next as *mut Option<K>,
            };
            ptr::swap(ptr_entry_next, ptr_prev_next);
            ptr::swap(ptr_entry_prev, ptr_next_prev);
        }
    }

    /// Inserts a new item into the Lru.
    /// The inserted entry will be placed at the end of the LRU list.
    /// If there was already an item with the same key, its value will be replaced with the new
    /// value and the old value is returned.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let result = match self.entries.entry(key.clone()) {
            hash_map::Entry::Occupied(mut o) => {
                // replace old value
                let old_value = mem::replace(&mut o.get_mut().data, value);

                // remove from linked list at the old position and re-insert at the end
                unsafe {
                    let ptr_entry = o.get_mut() as *mut Entry<K, V>;
                    self.raw_list_remove(ptr_entry);
                    self.raw_list_insert(ptr_entry, None);
                }

                // return whatever was previously stored under that key
                Some(old_value)
            }
            hash_map::Entry::Vacant(v) => {
                // insert
                let entry = Entry {
                    data: value,
                    next: Some(key.clone()),
                    prev: Some(key),
                };
                let ref_entry = v.insert(entry);

                // insert into linked list at the end
                unsafe {
                    let ptr_entry = ref_entry as *mut Entry<K, V>;
                    self.raw_list_insert(ptr_entry, None);
                }

                None
            }
        };

        result
    }

    /// Returns a reference to the value stored under the given key, if it exists.
    /// This will NOT change the item's position in the LRU order.
    /// Use [Lru::touch], if you want to also move it to the end of the LRU order.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|e| &e.data)
    }

    /// Moves the item with the given key to the end of the LRU order and returns a mutable
    /// reference to its value.
    pub fn touch(&mut self, key: &K) -> Option<&mut V> {
        // get the entry
        let ref_entry = self.entries.get_mut(key)?;

        unsafe {
            // remove from its old position in the linked list
            // and re-insert at the end
            let ptr_entry = ref_entry as *mut Entry<K, V>;
            self.raw_list_remove(ptr_entry);
            self.raw_list_insert(ptr_entry, None);

            // return reference to the stored value.
            // safety: the cast will construct a reference with the lifetime
            // that the function signature expects (elided lifetime).
            // So the returned mut reference will borrow self, as it should.
            Some(&mut (*ptr_entry).data)
        }
    }

    /// Removes the entry with the given key and returns the value, that it stored.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        // remove from hashmap
        let mut entry = self.entries.remove(key)?;

        // remove from linked list
        unsafe {
            let ptr_entry = &mut entry as *mut Entry<K, V>;
            self.raw_list_remove(ptr_entry);
        }

        // return old value
        Some(entry.data)
    }

    /// Returns an iterator over all entries.
    /// The entries are visited in the lru order,
    /// So entries that have recently been [insert]ed or [touch]ed will come last.
    pub fn iter(&self) -> Iter<K, V> {
        Iter {
            lru: self,
            next: self.first.clone(),
        }
    }

    /// Returns a mutable iterator over all entries.
    /// The entries are visited in the lru order,
    /// So entries that have recently been [insert]ed or [touch]ed will come last.
    pub fn iter_mut(&mut self) -> IterMut<K, V> {
        IterMut {
            next: self.first.clone(),
            lru: self,
        }
    }
}

pub struct Iter<'a, K, V> {
    lru: &'a Lru<K, V>,
    next: Option<K>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V>
where
    K: Hash + Eq + Clone,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match &self.next {
            None => None,
            Some(next_key) => {
                let (k, v) = self.lru.entries.get_key_value(next_key).unwrap();
                self.next.clone_from(&v.next);
                Some((k, &v.data))
            }
        }
    }
}

pub struct IterMut<'a, K, V> {
    lru: &'a mut Lru<K, V>,
    next: Option<K>,
}

impl<'a, K, V> Iterator for IterMut<'a, K, V>
where
    K: Hash + Eq + Clone,
{
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        match &self.next {
            None => None,
            Some(next_key) => {
                let occupied_entry = match self.lru.entries.entry(next_key.clone()) {
                    // on nightly, we could use the hash_raw_entry feature to avoid this clone...
                    hash_map::Entry::Occupied(o) => o,
                    hash_map::Entry::Vacant(_) => unreachable!(),
                };
                self.next.clone_from(&occupied_entry.get().next);

                // Cast references to key/value so their lifetimes directly borrow the lru cache.
                //
                // safety:
                // - unsafe, because we have to create two references into the
                //      self.lru.entries HashMap (one for the key, one for the value)
                //      of which one is a mut reference.
                //      It is however sound, because inside of the HashMap, the references will
                //      never point to the same target (one is a key and one is a value, after all.)
                // - unsafe, because the mut references to the values that this function
                //      creates all have lifetime 'a, and thus can overlap. This would allow the
                //      caller to create multiple mut references into the self.lru.entries HashMap.
                //      It is however sound, because the iterator will never visit an entry twice,
                //      so it is still impossible to create multiple references to the same value.
                unsafe {
                    // key
                    let k = occupied_entry.key();
                    let key_ptr = k as *const K;
                    let key_ref: &'a K = &*key_ptr;

                    // value
                    let v = occupied_entry.into_mut();
                    let data_ptr = &mut v.data as *mut V;
                    let data_ref: &'a mut V = &mut *data_ptr;
                    Some((key_ref, data_ref))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iter() {
        let mut lru = Lru::new();
        lru.insert(3, 1);
        lru.insert(1, 2);
        lru.insert(2, 3);
        lru.touch(&1);
        let items = lru.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>();
        assert_eq!(items, vec![(3, 1), (2, 3), (1, 2),]);
    }

    #[test]
    fn test_remove() {
        let mut lru = Lru::new();
        lru.insert(1, 1);
        lru.insert(2, 2);
        lru.insert(3, 3);
        lru.remove(&2);
        let items = lru.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>();
        assert_eq!(items, vec![(1, 1), (3, 3),]);
    }
}
