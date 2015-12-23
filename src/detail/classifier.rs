//! Classification of Pippin elements

use super::Element;
use pippin::Result;

/// A classifier should implement this trait.
/// 
/// Classifiers serve two purposes: they can be used as filters when searching
/// or listing elements, and they allow elements to be partitioned in a useful
/// manner.
/// 
/// Each classifier is allocated a 32-bit number as identifier within a
/// repository which never changes. It is desirable that this number stay
/// reasonably small, so do not continually add new classifiers.
trait Classifier {
    /// Return an identifier for the classifier. This should be reasonably short
    /// (ideally <= 8 bytes UTF-8, though not required) and not ever change.
    fn name(&self) -> &'static str;
    
    /// Run the classifier on an element and get an output value. The function
    /// should satisfy the following properties:
    /// 
    /// *   Be fixed. That is, for a given element which does not change, the
    ///     output should not vary (including on a new program load).
    /// *   Normally return a number of at least 1.
    /// *   As an exception, the special value 0 may be used. This indicates
    ///     a classification failure. The function may later return a non-zero
    ///     value, or return a non-zero value then later return 0 (but not any
    ///     other values).
    /// 
    /// Classifiers may be fine- or coarse-grained with any distribution.
    fn classify(&self, elt: &Element) -> u32;
    
    /// For elements where a classifier returns 0, a value may still be needed;
    /// this value gets used. For example, if a classifier returns the date at
    /// which elements are added, it may be useful to assume a future date for
    /// unclassified elements.
    /// 
    /// This value *shouldn't* change. If it does, elements may need to be
    /// moved to different partitions (increasing commit log size) and searches
    /// for "unclassified" elements may miss elements (since the wrong
    /// partitions may be checked).
    fn default_value(&self) -> u32;
    
    //TODO: is there any point allowing per-classifier control of caching?
    /// Control caching of the classifier value.
    /// 
    /// If true, the classifier value will be added at the time of element
    /// creation and added. Elements without classifier may or may not be
    /// updated with it. The `classify` function will not be used on elements
    /// with a cached value, e.g. when searching and repartitioning, excepting
    /// if the cached value is 0.
    /// 
    /// TODO: add a way to force updating of cached values (though in theory
    /// it should not be needed).
    /// 
    /// If false, the `classify` function will be called whenever the value is
    /// needed. If elements have a cached value this will be ignored (normally
    /// it will not be removed, though this is not guaranteed).
    /// 
    /// It is allowable to turn caching on or off. It is not recommended to
    /// turn caching off except where the `classify` function is quite fast
    /// (mostly due to the performance of searches).
    fn use_caching(&self) -> bool;
}

/// Type used to represent a set of classifiers used in a repository.
pub struct Classifiers {
    unnumbered_classifiers: Vec<&'static Classifier>,
    classifiers: VecMap<&'static Classifier>,
};

impl Classifiers {
    /// Classify an element.
    fn classify(&self, elt: &Element, classifier: u32) -> Result<u32> {
        if !self.classifiers.has_key(classifier) {
            return Err("invalid classifier");
        }
        let cached = elt.cached_classification(classifier);
        if cached == 0 {
            let classifier = self.classifiers.get(classifier).unwrap();
            let csf = classifier.classify(elt);
            if csf == 0 {
                classifier.default_value()
            } else {
                //TODO: update cache in element? Or maybe only when snapshotting?
                csf
            }
        } else {
            cached
        }
    }
}
