//! Set CRDTs: Observed-Remove Set (OR-Set).
//!
//! A distributed set is tricky because concurrent `add` and `remove` of the
//! same element conflict. OR-Set resolves this by tagging each `add` with a
//! unique token, and only allowing `remove` to delete tags it has actually
//! observed. This gives **add-wins** semantics: a concurrent add survives a remove.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::{clock::LamportTimestamp, Crdt};

/// An Observed-Remove Set.
///
/// # Concept
///
/// Each `add(x)` creates a unique tag (a `LamportTimestamp`). The set stores
/// a map from elements to their tags, and a set of tombstoned tags. An element
/// is considered present if it has at least one tag that is not a tombstone.
///
/// `remove(x)` moves all current tags for `x` into tombstones. A concurrent
/// `add` from another replica uses a new tag that this replica hasn't seen, so
/// the remove cannot touch it — the element survives the merge.
///
/// # Example
///
/// ```
/// use crdt_lib::clock::LamportClock;
/// use crdt_lib::set::OrSet;
/// use crdt_lib::Crdt;
///
/// let mut clock = LamportClock::new(1);
/// let mut s = OrSet::new();
/// s.add("milk", clock.tick());
/// s.add("bread", clock.tick());
/// assert!(s.contains(&"milk"));
///
/// s.remove(&"milk");
/// assert!(!s.contains(&"milk"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrSet<T>
where
    T: Ord + Clone,
{
    /// Per-element set of tags that have been added.
    tags: BTreeMap<T, BTreeSet<LamportTimestamp>>,
    /// Tags that have been removed (tombstones).
    tombstones: BTreeSet<LamportTimestamp>,
}

impl<T> Default for OrSet<T>
where
    T: Ord + Clone,
{
    fn default() -> Self {
        Self {
            tags: BTreeMap::new(),
            tombstones: BTreeSet::new(),
        }
    }
}

impl<T> OrSet<T>
where
    T: Ord + Clone + Hash,
{
    /// Creates an empty OR-Set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `value` with a unique tag.
    ///
    /// The tag must be globally unique (typically from a `LamportClock`).
    pub fn add(&mut self, value: T, tag: LamportTimestamp) {
        self.tags.entry(value).or_default().insert(tag);
    }

    /// Removes `value` by tombstoning all its currently observed tags.
    ///
    /// Tags added concurrently by other replicas (not yet visible to this
    /// replica) will not be touched and will survive the merge.
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.tags.get(value) {
            for tag in tags {
                self.tombstones.insert(*tag);
            }
        }
    }

    /// Returns `true` if `value` has at least one non-tombstoned tag.
    #[must_use]
    pub fn contains(&self, value: &T) -> bool {
        self.tags
            .get(value)
            .is_some_and(|tags| tags.iter().any(|tag| !self.tombstones.contains(tag)))
    }

    /// Returns an iterator over all elements currently in the set.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.tags
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.tombstones.contains(tag)))
            .map(|(value, _)| value)
    }

    /// Returns the number of elements in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Returns `true` if the set contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<T> Crdt for OrSet<T>
where
    T: Ord + Clone + Hash,
{
    fn merge(&mut self, other: &Self) {
        // Union of tags per element.
        for (value, other_tags) in &other.tags {
            self.tags
                .entry(value.clone())
                .or_default()
                .extend(other_tags.iter().copied());
        }
        // Union of tombstones.
        self.tombstones.extend(other.tombstones.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;

    #[test]
    fn new_set_is_empty() {
        let s: OrSet<&str> = OrSet::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn add_makes_element_present() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        assert!(s.contains(&"a"));
    }

    #[test]
    fn remove_makes_element_absent() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.remove(&"a");
        assert!(!s.contains(&"a"));
    }

    #[test]
    fn re_add_after_remove_works() {
        // After a remove, the element can be re-added with a new tag.
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.remove(&"a");
        s.add("a", clock.tick()); // new tag, not tombstoned
        assert!(s.contains(&"a"));
    }

    #[test]
    fn concurrent_add_wins_over_remove() {
        // Core OR-Set semantics.
        // Replica 1 adds then removes.
        let mut clock1 = LamportClock::new(1);
        let mut a = OrSet::new();
        let tag1 = clock1.tick();
        a.add("x", tag1);
        a.remove(&"x");

        // Replica 2 starts from the same initial state but adds a new instance
        // concurrently, without seeing replica 1's remove.
        let mut clock2 = LamportClock::new(2);
        let mut b = OrSet::new();
        b.add("x", tag1); // saw the original add
        b.add("x", clock2.tick()); // concurrent add with a new tag

        // Merge: a's remove only tombstones tag1; b's new tag survives.
        a.merge(&b);
        assert!(a.contains(&"x"));
    }

    #[test]
    fn merge_is_symmetric_for_concurrent_add_remove() {
        // Same scenario as above, but verifies merge order doesn't matter.
        let mut clock1 = LamportClock::new(1);
        let mut clock2 = LamportClock::new(2);

        let tag1 = clock1.tick();

        let mut a = OrSet::new();
        a.add("x", tag1);
        a.remove(&"x");

        let mut b = OrSet::new();
        b.add("x", tag1);
        b.add("x", clock2.tick());

        let mut a_then_b = a.clone();
        a_then_b.merge(&b);

        let mut b_then_a = b.clone();
        b_then_a.merge(&a);

        assert_eq!(a_then_b, b_then_a);
    }

    #[test]
    fn iter_yields_present_elements() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.add("b", clock.tick());
        s.add("c", clock.tick());
        s.remove(&"b");

        let mut present: Vec<&&str> = s.iter().collect();
        present.sort();
        assert_eq!(present, vec![&"a", &"c"]);
    }
}
