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
use std::borrow::Borrow;

/// Configures how values are indexd.
/// 
/// User should either implement extract_key() or implement key_eq() and
/// key_hash() (in the latter case, extract_key() technically needs an
/// implementation but will never be used, so it can simply panic).
/// 
/// Note that `get` and `remove` require implementation of extract_key() in
/// order to function correctly!
pub trait KeyComparator<T, K> where K: Eq + Hash {
    /// This function should return a key extracted from the value.
    /// `eq` and `hash` are implemented on this key.
    /// 
    /// Note that the implementation could simply panic if `key_eq()` and
    /// `key_hash()` are implemented instead; however `get()` and `remove()`
    /// will not work in this case.
    fn extract_key(value: &T) -> &K;
    
    /// Test equality of keys extracted from given values u, v.
    fn key_eq(u: &T, v: &T) -> bool {
        Self::extract_key(u) == Self::extract_key(v)
    }
    
    /// Generate a hash of a key retrieved from a given value.
    fn key_hash<H: Hasher>(value: &T, state: &mut H) {
        Self::extract_key(value).hash(state)
    }
}

/// Internal type
pub struct IndexableValue<T, K, E> {
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
    where E: KeyComparator<T, K>, K: Eq + Hash
{
    fn eq(&self, other: &IndexableValue<T, K, E>) -> bool {
        E::key_eq(&self.value, &other.value)
    }
}
impl<T, K, E> Eq for IndexableValue<T, K, E> where E: KeyComparator<T, K>, K: Eq + Hash {}
impl<T, K, E> Hash for IndexableValue<T, K, E> where E: KeyComparator<T, K>, K: Eq + Hash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        E::key_hash(&self.value, state)
    }
}
impl<T, K, E> Borrow<K> for IndexableValue<T, K, E>
    where E: KeyComparator<T, K>, K: Eq + Hash
{
    fn borrow(&self) -> &K {
        E::extract_key(&self.value)
    }
}


/// Stores a set of values indexed in a user-defined way.
/// 
/// Use like this:
/// 
///     use pippin::hashindexed::{HashIndexed, KeyComparator};
///     
///     struct MyType { num: i32, name: &'static str }
///     
///     struct MyComparator;
///     impl KeyComparator<MyType, i32> for MyComparator {
///         fn extract_key(v: &MyType) -> &i32 { &v.num }
///     }
///     
///     let mut container: HashIndexed<MyType, i32, MyComparator> = HashIndexed::new();
///     container.insert(MyType { num: 1, name: "one" });
///     assert_eq!( container.remove(&1).unwrap().name, "one" );
pub struct HashIndexed<T, K, E> {
    set: HashSet<IndexableValue<T, K, E>>
}

impl<T, K, E> HashIndexed<T, K, E>
    where E: KeyComparator<T, K>, K: Eq + Hash,
    IndexableValue<T, K, E>: Borrow<K>
{
    /// Creates an empty HashIndexed collection.
    pub fn new() -> HashIndexed<T, K, E> {
        HashIndexed { set: HashSet::new() }
    }
    
    /// Returns a reference to the value corresponding to the key.
    pub fn get(&self, k: &K) -> Option<&T> {
        self.set.get(k).map(|v| &v.value)
    }
    
    /// Removes and returns the value in the collection, if any, that is equal
    /// to the given one.
    pub fn remove(&mut self, k: &K) -> Option<T> {
        // Note that 'take' in HashSet corresponds to 'remove' for values in
        // HashMap; this is because it was added after API stabilisation of the
        // existing 'remove' function.
        self.set.take(k).map(|v| v.value)
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
    where K: Eq + Hash, E: KeyComparator<T, K>,
    IndexableValue<T, K, E>: Borrow<K>
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T, K, E>;
    fn into_iter(self) -> Iter<'a, T, K, E> {
        self.iter()
    }
}

impl<'a, T, K, E> IntoIterator for HashIndexed<T, K, E>
    where K: Eq + Hash, E: KeyComparator<T, K>
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
