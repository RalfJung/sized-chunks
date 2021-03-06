// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::fmt::Error;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::IndexMut;
use std::ops::{Bound, Index, Range, RangeBounds};

use crate::types::ChunkLength;

use super::{Iter, IterMut, RingBuffer};

/// An indexable representation of a subset of a `RingBuffer`.
pub struct Slice<'a, A, N: ChunkLength<A>> {
    pub(crate) buffer: &'a RingBuffer<A, N>,
    pub(crate) range: Range<usize>,
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> Slice<'a, A, N> {
    /// Get the length of the slice.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Test if the slice is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a reference to the value at a given index.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&A> {
        if index >= self.len() {
            None
        } else {
            Some(unsafe { self.get_unchecked(index) })
        }
    }

    /// Get an unchecked reference to the value at the given index.
    ///
    /// # Safety
    ///
    /// You must ensure the index is not out of bounds.
    #[must_use]
    pub unsafe fn get_unchecked(&self, index: usize) -> &A {
        self.buffer.get_unchecked(self.range.start + index)
    }

    /// Get a reference to the first value in the slice.
    #[inline]
    #[must_use]
    pub fn first(&self) -> Option<&A> {
        self.get(0)
    }

    /// Get a reference to the last value in the slice.
    #[inline]
    #[must_use]
    pub fn last(&self) -> Option<&A> {
        if self.is_empty() {
            None
        } else {
            self.get(self.len() - 1)
        }
    }

    /// Get an iterator over references to the items in the slice in order.
    #[inline]
    #[must_use]
    pub fn iter(&self) -> Iter<'_, A, N> {
        Iter {
            buffer: self.buffer,
            left_index: self.buffer.origin + self.range.start,
            right_index: self.buffer.origin + self.range.start + self.len(),
            remaining: self.len(),
        }
    }

    /// Create a subslice of this slice.
    ///
    /// This consumes the slice. To create a subslice without consuming it,
    /// clone it first: `my_slice.clone().slice(1..2)`.
    #[must_use]
    pub fn slice<R: RangeBounds<usize>>(self, range: R) -> Slice<'a, A, N> {
        let new_range = Range {
            start: match range.start_bound() {
                Bound::Unbounded => self.range.start,
                Bound::Included(index) => self.range.start + index,
                Bound::Excluded(_) => unimplemented!(),
            },
            end: match range.end_bound() {
                Bound::Unbounded => self.range.end,
                Bound::Included(index) => self.range.start + index + 1,
                Bound::Excluded(index) => self.range.start + index,
            },
        };
        if new_range.start < self.range.start
            || new_range.end > self.range.end
            || new_range.start > new_range.end
        {
            panic!("Slice::slice: index out of bounds");
        }
        Slice {
            buffer: self.buffer,
            range: new_range,
        }
    }

    /// Split the slice into two subslices at the given index.
    #[must_use]
    pub fn split_at(self, index: usize) -> (Slice<'a, A, N>, Slice<'a, A, N>) {
        if index > self.len() {
            panic!("Slice::split_at: index out of bounds");
        }
        let index = self.range.start + index;
        (
            Slice {
                buffer: self.buffer,
                range: Range {
                    start: self.range.start,
                    end: index,
                },
            },
            Slice {
                buffer: self.buffer,
                range: Range {
                    start: index,
                    end: self.range.end,
                },
            },
        )
    }

    /// Construct a new `RingBuffer` by copying the elements in this slice.
    #[inline]
    #[must_use]
    pub fn to_owned(&self) -> RingBuffer<A, N>
    where
        A: Clone,
    {
        self.iter().cloned().collect()
    }

    /// Perform a binary search for the given value.
    ///
    /// This assumes the slice contains ordered data.
    pub fn binary_search(&self, target: &A) -> Result<usize, usize>
    where
        A: Ord,
    {
        self.binary_search_by(|value| value.cmp(target))
    }

    /// Perform a binary search using the given comparator function.
    ///
    /// This assumes the slice contains ordered data.
    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(&A) -> Ordering,
    {
        let s = self;
        let mut size = s.len();
        if size == 0 {
            return Err(0);
        }
        let mut base = 0usize;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            let cmp = f(unsafe { s.get_unchecked(mid) });
            base = if cmp == Ordering::Greater { base } else { mid };
            size -= half;
        }
        let cmp = f(unsafe { s.get_unchecked(base) });
        if cmp == Ordering::Equal {
            Ok(base)
        } else {
            Err(base + (cmp == Ordering::Less) as usize)
        }
    }

    /// Perform a binary search with a key extraction function.
    ///
    /// This assumes the slice contains data ordered by the key.
    pub fn binary_search_by_key<K, F>(&self, b: &K, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(&A) -> K,
        K: Ord,
    {
        self.binary_search_by(|k| f(k).cmp(b))
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> From<&'a RingBuffer<A, N>> for Slice<'a, A, N> {
    #[inline]
    #[must_use]
    fn from(buffer: &'a RingBuffer<A, N>) -> Self {
        Slice {
            range: Range {
                start: 0,
                end: buffer.len(),
            },
            buffer,
        }
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> Clone for Slice<'a, A, N> {
    #[inline]
    #[must_use]
    fn clone(&self) -> Self {
        Slice {
            buffer: self.buffer,
            range: self.range.clone(),
        }
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> Index<usize> for Slice<'a, A, N> {
    type Output = A;

    #[inline]
    #[must_use]
    fn index(&self, index: usize) -> &Self::Output {
        self.buffer.index(self.range.start + index)
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq for Slice<'a, A, N> {
    #[inline]
    #[must_use]
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq<SliceMut<'a, A, N>>
    for Slice<'a, A, N>
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &SliceMut<'a, A, N>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq<RingBuffer<A, N>>
    for Slice<'a, A, N>
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &RingBuffer<A, N>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a, S> PartialEq<S> for Slice<'a, A, N>
where
    S: Borrow<[A]>,
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &S) -> bool {
        let other = other.borrow();
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: Eq + 'a, N: ChunkLength<A> + 'a> Eq for Slice<'a, A, N> {}

impl<'a, A: PartialOrd + 'a, N: ChunkLength<A> + 'a> PartialOrd for Slice<'a, A, N> {
    #[inline]
    #[must_use]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<'a, A: Ord + 'a, N: ChunkLength<A> + 'a> Ord for Slice<'a, A, N> {
    #[inline]
    #[must_use]
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<'a, A: Debug + 'a, N: ChunkLength<A> + 'a> Debug for Slice<'a, A, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.write_str("RingBuffer")?;
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, A: Hash + 'a, N: ChunkLength<A> + 'a> Hash for Slice<'a, A, N> {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        for item in self {
            item.hash(hasher)
        }
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> IntoIterator for &'a Slice<'a, A, N> {
    type Item = &'a A;
    type IntoIter = Iter<'a, A, N>;

    #[inline]
    #[must_use]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// Mutable slice

/// An indexable representation of a mutable subset of a `RingBuffer`.
pub struct SliceMut<'a, A, N: ChunkLength<A>> {
    pub(crate) buffer: &'a mut RingBuffer<A, N>,
    pub(crate) range: Range<usize>,
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> SliceMut<'a, A, N> {
    /// Downgrade this slice into a non-mutable slice.
    #[inline]
    #[must_use]
    pub fn unmut(self) -> Slice<'a, A, N> {
        Slice {
            buffer: self.buffer,
            range: self.range,
        }
    }

    /// Get the length of the slice.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Test if the slice is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a reference to the value at a given index.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&A> {
        if index >= self.len() {
            None
        } else {
            Some(unsafe { self.get_unchecked(index) })
        }
    }

    /// Get an unchecked reference to the value at the given index.
    ///
    /// # Safety
    ///
    /// You must ensure the index is not out of bounds.
    #[must_use]
    pub unsafe fn get_unchecked(&self, index: usize) -> &A {
        self.buffer.get_unchecked(self.range.start + index)
    }

    /// Get a mutable reference to the value at a given index.
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut A> {
        if index >= self.len() {
            None
        } else {
            Some(unsafe { self.get_unchecked_mut(index) })
        }
    }

    /// Get an unchecked mutable reference to the value at the given index.
    ///
    /// # Safety
    ///
    /// You must ensure the index is not out of bounds.
    #[must_use]
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> &mut A {
        self.buffer.get_unchecked_mut(self.range.start + index)
    }

    /// Get a reference to the first value in the slice.
    #[inline]
    #[must_use]
    pub fn first(&self) -> Option<&A> {
        self.get(0)
    }

    /// Get a mutable reference to the first value in the slice.
    #[inline]
    #[must_use]
    pub fn first_mut(&mut self) -> Option<&mut A> {
        self.get_mut(0)
    }

    /// Get a reference to the last value in the slice.
    #[inline]
    #[must_use]
    pub fn last(&self) -> Option<&A> {
        if self.is_empty() {
            None
        } else {
            self.get(self.len() - 1)
        }
    }

    /// Get a mutable reference to the last value in the slice.
    #[inline]
    #[must_use]
    pub fn last_mut(&mut self) -> Option<&mut A> {
        if self.is_empty() {
            None
        } else {
            self.get_mut(self.len() - 1)
        }
    }

    /// Get an iterator over references to the items in the slice in order.
    #[inline]
    #[must_use]
    pub fn iter(&self) -> Iter<'_, A, N> {
        Iter {
            buffer: self.buffer,
            left_index: self.buffer.origin + self.range.start,
            right_index: self.buffer.origin + self.range.start + self.len(),
            remaining: self.len(),
        }
    }

    /// Get an iterator over mutable references to the items in the slice in
    /// order.
    #[inline]
    #[must_use]
    pub fn iter_mut(&mut self) -> IterMut<'_, A, N> {
        IterMut::new_slice(
            self.buffer,
            self.buffer.origin + self.range.start,
            self.len(),
        )
    }

    /// Create a subslice of this slice.
    ///
    /// This consumes the slice. Because the slice works like a mutable
    /// reference, you can only have one slice over a given subset of a
    /// `RingBuffer` at any one time, so that's just how it's got to be.
    #[must_use]
    pub fn slice<R: RangeBounds<usize>>(self, range: R) -> SliceMut<'a, A, N> {
        let new_range = Range {
            start: match range.start_bound() {
                Bound::Unbounded => self.range.start,
                Bound::Included(index) => self.range.start + index,
                Bound::Excluded(_) => unimplemented!(),
            },
            end: match range.end_bound() {
                Bound::Unbounded => self.range.end,
                Bound::Included(index) => self.range.start + index + 1,
                Bound::Excluded(index) => self.range.start + index,
            },
        };
        if new_range.start < self.range.start
            || new_range.end > self.range.end
            || new_range.start > new_range.end
        {
            panic!("Slice::slice: index out of bounds");
        }
        SliceMut {
            buffer: self.buffer,
            range: new_range,
        }
    }

    /// Split the slice into two subslices at the given index.
    #[must_use]
    pub fn split_at(self, index: usize) -> (SliceMut<'a, A, N>, SliceMut<'a, A, N>) {
        if index > self.len() {
            panic!("SliceMut::split_at: index out of bounds");
        }
        let index = self.range.start + index;
        let ptr: *mut RingBuffer<A, N> = self.buffer;
        (
            SliceMut {
                buffer: unsafe { &mut *ptr },
                range: Range {
                    start: self.range.start,
                    end: index,
                },
            },
            SliceMut {
                buffer: unsafe { &mut *ptr },
                range: Range {
                    start: index,
                    end: self.range.end,
                },
            },
        )
    }

    /// Update the value at index `index`, returning the old value.
    ///
    /// Panics if `index` is out of bounds.
    #[inline]
    #[must_use]
    pub fn set(&mut self, index: usize, value: A) -> A {
        if index >= self.len() {
            panic!("SliceMut::set: index out of bounds");
        } else {
            self.buffer.set(self.range.start + index, value)
        }
    }

    /// Construct a new `RingBuffer` by copying the elements in this slice.
    #[inline]
    #[must_use]
    pub fn to_owned(&self) -> RingBuffer<A, N>
    where
        A: Clone,
    {
        self.iter().cloned().collect()
    }

    /// Perform a binary search for the given value.
    ///
    /// This assumes the slice contains ordered data.
    pub fn binary_search(&self, target: &A) -> Result<usize, usize>
    where
        A: Ord,
    {
        self.binary_search_by(|value| value.cmp(target))
    }

    /// Perform a binary search using the given comparator function.
    ///
    /// This assumes the slice contains ordered data.
    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(&A) -> Ordering,
    {
        let s = self;
        let mut size = s.len();
        if size == 0 {
            return Err(0);
        }
        let mut base = 0usize;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            let cmp = f(unsafe { s.get_unchecked(mid) });
            base = if cmp == Ordering::Greater { base } else { mid };
            size -= half;
        }
        let cmp = f(unsafe { s.get_unchecked(base) });
        if cmp == Ordering::Equal {
            Ok(base)
        } else {
            Err(base + (cmp == Ordering::Less) as usize)
        }
    }

    /// Perform a binary search with a key extraction function.
    ///
    /// This assumes the slice contains data ordered by the key.
    pub fn binary_search_by_key<K, F>(&self, b: &K, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(&A) -> K,
        K: Ord,
    {
        self.binary_search_by(|k| f(k).cmp(b))
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> From<&'a mut RingBuffer<A, N>> for SliceMut<'a, A, N> {
    #[must_use]
    fn from(buffer: &'a mut RingBuffer<A, N>) -> Self {
        SliceMut {
            range: Range {
                start: 0,
                end: buffer.len(),
            },
            buffer,
        }
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> Into<Slice<'a, A, N>> for SliceMut<'a, A, N> {
    #[inline]
    #[must_use]
    fn into(self) -> Slice<'a, A, N> {
        self.unmut()
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> Index<usize> for SliceMut<'a, A, N> {
    type Output = A;

    #[inline]
    #[must_use]
    fn index(&self, index: usize) -> &Self::Output {
        self.buffer.index(self.range.start + index)
    }
}

impl<'a, A: 'a, N: ChunkLength<A> + 'a> IndexMut<usize> for SliceMut<'a, A, N> {
    #[inline]
    #[must_use]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.buffer.index_mut(self.range.start + index)
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq for SliceMut<'a, A, N> {
    #[inline]
    #[must_use]
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq<Slice<'a, A, N>>
    for SliceMut<'a, A, N>
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &Slice<'a, A, N>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a> PartialEq<RingBuffer<A, N>>
    for SliceMut<'a, A, N>
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &RingBuffer<A, N>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: PartialEq + 'a, N: ChunkLength<A> + 'a, S> PartialEq<S> for SliceMut<'a, A, N>
where
    S: Borrow<[A]>,
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &S) -> bool {
        let other = other.borrow();
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<'a, A: Eq + 'a, N: ChunkLength<A> + 'a> Eq for SliceMut<'a, A, N> {}

impl<'a, A: PartialOrd + 'a, N: ChunkLength<A> + 'a> PartialOrd for SliceMut<'a, A, N> {
    #[inline]
    #[must_use]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<'a, A: Ord + 'a, N: ChunkLength<A> + 'a> Ord for SliceMut<'a, A, N> {
    #[inline]
    #[must_use]
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<'a, A: Debug + 'a, N: ChunkLength<A> + 'a> Debug for SliceMut<'a, A, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.write_str("RingBuffer")?;
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, A: Hash + 'a, N: ChunkLength<A> + 'a> Hash for SliceMut<'a, A, N> {
    #[inline]
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        for item in self {
            item.hash(hasher)
        }
    }
}

impl<'a, 'b, A: 'a, N: ChunkLength<A> + 'a> IntoIterator for &'a SliceMut<'a, A, N> {
    type Item = &'a A;
    type IntoIter = Iter<'a, A, N>;

    #[inline]
    #[must_use]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'b, A: 'a, N: ChunkLength<A> + 'a> IntoIterator for &'a mut SliceMut<'a, A, N> {
    type Item = &'a mut A;
    type IntoIter = IterMut<'a, A, N>;

    #[inline]
    #[must_use]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
