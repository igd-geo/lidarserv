use std::{
    mem::{transmute, MaybeUninit},
    ops::{Deref, DerefMut},
};

use serde::{de::Visitor, Deserialize, Serialize};

/// A 'Vec' with a fixed capacity, allocated on the stack.
pub struct FixedVec<T, const CAP: usize> {
    len: usize,
    data: [MaybeUninit<T>; CAP],
}

impl<T, const CAP: usize> FixedVec<T, CAP> {
    pub const fn new() -> Self {
        FixedVec {
            len: 0,
            data: [const { MaybeUninit::uninit() }; CAP],
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn is_full(&self) -> bool {
        self.len == CAP
    }

    pub fn push(&mut self, value: T) {
        assert!(self.len < CAP);
        self.data[self.len].write(value);
        self.len += 1;
    }

    pub fn clear(&mut self) {
        for i in 0..self.len {
            // safety: values within len are initialized.
            unsafe { self.data[i].assume_init_drop() };
        }
        self.len = 0;
    }

    /// removes consecutive duplicates.
    /// the slice must be sorted for this to work!
    pub fn dedup(&mut self)
    where
        T: Eq,
    {
        let mut rd = 1;
        let mut wr = 1;

        // safety:
        // after each loop iteration, the following conditions hold:
        // 0..wr: init
        // wr..rd: uninit
        // rd..self.len: init
        //
        // therefore:
        // self.data[rd]: initialized
        // self.data[wr - 1]: initialized
        // self.data[wr]: uninitialized (if rd == wr, this will be uninitialized after the value has been read from self.data[rd])
        unsafe {
            while rd < self.len {
                let value = self.data[rd].assume_init_read();
                rd += 1;
                if value != *self.data[wr - 1].assume_init_ref() {
                    self.data[wr].write(value);
                    wr += 1;
                }
            }
        }
        self.len = wr;
    }
}

impl<T, const CAP: usize> Default for FixedVec<T, CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CAP: usize> Drop for FixedVec<T, CAP> {
    fn drop(&mut self) {
        // ensure remaining items are dropped.
        self.clear();
    }
}

impl<T, const CAP: usize> Serialize for FixedVec<T, CAP>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.deref().serialize(serializer)
    }
}

impl<'de, T, const CAP: usize> Deserialize<'de> for FixedVec<T, CAP>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SeqVisitor<T2, const CAP2: usize> {
            result: FixedVec<T2, CAP2>,
        }

        impl<'de2, T2: Deserialize<'de2>, const CAP2: usize> Visitor<'de2> for SeqVisitor<T2, CAP2> {
            type Value = FixedVec<T2, CAP2>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a list with 0 to {CAP2} elements.")
            }

            fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de2>,
            {
                while let Some(elem) = seq.next_element::<T2>()? {
                    if self.result.is_full() {
                        return Err(serde::de::Error::invalid_length(
                            self.result.len() + 1,
                            &format!("a list with up to {CAP2} elements.").as_str(),
                        ));
                    }
                    self.result.push(elem);
                }
                Ok(self.result)
            }
        }
        deserializer.deserialize_seq(SeqVisitor::<T, CAP> {
            result: FixedVec::new(),
        })
    }
}

impl<T, const CAP: usize> Deref for FixedVec<T, CAP> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        let init = &self.data[..self.len];

        // safety:
        // [MaybeUninit<T>; LEN] and [T; LEN] have identical layouts
        // and we know that all elements in the slice (0 - len) are initialized.
        unsafe { transmute(init) }
    }
}

impl<T, const CAP: usize> DerefMut for FixedVec<T, CAP> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        let init = &mut self.data[..self.len];

        // safety:
        // [MaybeUninit<T>; LEN] and [T; LEN] have identical layouts
        // and we know that all elements in the slice (0 - len) are initialized.
        unsafe { transmute(init) }
    }
}

impl<T, const CAP: usize> Clone for FixedVec<T, CAP>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut result = FixedVec::new();
        while result.len < self.len {
            result.push(self[result.len].clone());
        }
        result
    }
}

/// IntoIterator implementation for [FixedVec].
pub struct IntoIter<T, const CAP: usize> {
    pos: usize,
    len: usize,
    data: [MaybeUninit<T>; CAP],
}

impl<T, const CAP: usize> Iterator for IntoIter<T, CAP> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.len {
            // safety: slice (self.pos - self.len) is always initialized.
            let elem = unsafe { self.data[self.pos].assume_init_read() };
            self.pos += 1;
            Some(elem)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let s = self.len - self.pos;
        (s, Some(s))
    }
}

impl<T, const CAP: usize> Drop for IntoIter<T, CAP> {
    fn drop(&mut self) {
        // drop all items, that have not been consumed.
        for i in self.pos..self.len {
            // safety: slice between self.pos and self.len is initialized.
            unsafe { self.data[i].assume_init_drop() };
        }
        self.pos = self.len;
    }
}

impl<T, const CAP: usize> IntoIterator for FixedVec<T, CAP> {
    type Item = T;

    type IntoIter = IntoIter<T, CAP>;

    fn into_iter(mut self) -> Self::IntoIter {
        let len = std::mem::replace(&mut self.len, 0);
        let data = std::mem::replace(&mut self.data, [const { MaybeUninit::uninit() }; CAP]);
        IntoIter { pos: 0, len, data }
    }
}

impl<'a, T, const CAP: usize> IntoIterator for &'a FixedVec<T, CAP> {
    type Item = &'a T;

    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_ref().iter()
    }
}

impl<'a, T, const CAP: usize> IntoIterator for &'a mut FixedVec<T, CAP> {
    type Item = &'a mut T;

    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_mut().iter_mut()
    }
}
