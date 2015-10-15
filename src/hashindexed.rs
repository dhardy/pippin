//! Store a set of values in a data structure indexed by the hash of some
//! user-defined sub-property.
//! 
//! This works like a HashSet<T> with redefined equality and hash function on
//! T, but maintaining the usual definition of equality on T outside the
//! indexing.

use std::collections::HashSet;
use std::collections::hash_set;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

pub trait KeyExtractor<T, K> {
    fn extract_key(value: &T) -> &K;
}

struct IndexableValue<T, K, E> {
    phantom_k: PhantomData<K>,
    phantom_e: PhantomData<E>,
    value: T
}
impl<T, K, E> IndexableValue<T, K, E> {
    fn new(value: T) -> IndexableValue<T, K, E> {
        IndexableValue {
            phantom_k: PhantomData,
            phantom_e: PhantomData,
            value: value
        }
    }
}

impl<T, K, E> PartialEq<IndexableValue<T, K, E>> for IndexableValue<T, K, E>
    where E: KeyExtractor<T, K>, K: Eq
{
    fn eq(&self, other: &IndexableValue<T, K, E>) -> bool {
        E::extract_key(&self.value) == E::extract_key(&other.value)
    }
}
impl<T, K, E> Eq for IndexableValue<T, K, E> where E: KeyExtractor<T, K>, K: Eq {}
impl<T, K, E> Hash for IndexableValue<T, K, E> where E: KeyExtractor<T, K>, K: Hash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        E::extract_key(&self.value).hash(state)
    }
}


/// Stores a set of values indexed in a user-defined way.
pub struct HashIndexed<T, K, E> {
    set: HashSet<IndexableValue<T, K, E>>
}

impl<T, K, E> HashIndexed<T, K, E> where E: KeyExtractor<T, K>, K: Eq + Hash {
    /// Creates an empty HashIndexed collection.
    pub fn new() -> HashIndexed<T, K, E> {
        HashIndexed { set: HashSet::new() }
    }
    
    /// Adds a value to the set. Returns true if the value was not already
    /// present in the collection.
    pub fn insert(&mut self, value: T) -> bool {
        self.set.insert(IndexableValue::new(value))
    }
    
    /// An iterator visiting all elements in arbitrary order.
    pub fn iter(&self) -> Iter<T, K, E> {
        Iter { iter: self.set.iter() }
    }
    
    /// Creates a consuming iterator, that is, one that moves each value out
    /// of the set in arbitrary order. The set cannot be used after calling
    /// this.
    pub fn into_iter(self) -> IntoIter<T, K, E> {
        IntoIter { iter: self.set.into_iter() }
    }
}

/// HashIndexed iterator
pub struct Iter<'a, T: 'a, K: 'a, E: 'a> {
    iter: hash_set::Iter<'a, IndexableValue<T, K, E>>
}

/// HashIndexed move iterator
pub struct IntoIter<T, K, E> {
    iter: hash_set::IntoIter<IndexableValue<T, K, E>>
}

impl<'a, T, K, E> IntoIterator for &'a HashIndexed<T, K, E>
    where K: Eq + Hash, E: KeyExtractor<T, K>
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T, K, E>;
    fn into_iter(self) -> Iter<'a, T, K, E> {
        self.iter()
    }
}

impl<'a, T, K, E> IntoIterator for HashIndexed<T, K, E>
    where K: Eq + Hash, E: KeyExtractor<T, K>
{
    type Item = T;
    type IntoIter = IntoIter<T, K, E>;
    fn into_iter(self) -> IntoIter<T, K, E> {
        self.into_iter()
    }
}

impl<'a, T, K, E> Iterator for Iter<'a, T, K, E> {
    type Item = &'a T;
    
    fn next(&mut self) -> Option<&'a T> { self.iter.next().map(|x| &x.value) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
impl<'a, T, K, E> ExactSizeIterator for Iter<'a, T, K, E> {
    fn len(&self) -> usize { self.iter.len() }
}

impl<T, K, E> Iterator for IntoIter<T, K, E> {
    type Item = T;

    fn next(&mut self) -> Option<T> { self.iter.next().map(|x| x.value) }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
impl<T, K, E> ExactSizeIterator for IntoIter<T, K, E> {
    fn len(&self) -> usize { self.iter.len() }
}
