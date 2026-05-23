//! Register CRDTs: Last-Writer-Wins Register.
//!
//! A register holds a single value. In a distributed system, multiple replicas
//! may write concurrently; an LWW Register resolves conflicts by keeping the
//! write with the highest Lamport timestamp.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{clock::LamportTimestamp, Crdt};

/// A Last-Writer-Wins register.
///
/// # Concept
///
/// State is either empty or a `(value, timestamp)` pair. On merge, the entry
/// with the higher `LamportTimestamp` wins. Because `LamportTimestamp` has a
/// total order (`counter`, then `replica_id`), all replicas always agree on
/// the winner.
///
/// Concurrent writes discard one value — use [`crate::set::OrSet`] if you
/// need accumulating semantics (e.g. a shopping cart).
///

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T> {
    entry: Option<Entry<T>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry<T> {
    value: T,
    timestamp: LamportTimestamp,
}

impl<T> Default for LwwRegister<T> {
    fn default() -> Self {
        Self { entry: None }
    }
}

impl<T> LwwRegister<T> {
    /// Creates an empty register.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes a value with the given timestamp.
    ///
    /// The write is ignored if its timestamp does not exceed the current one,
    /// preserving CRDT properties under message reordering.
    pub fn write(&mut self, value: T, timestamp: LamportTimestamp) {
        match &self.entry {
            Some(current) if current.timestamp >= timestamp => {
                // Incoming write is not newer; keep the existing value.
            }
            _ => {
                self.entry = Some(Entry { value, timestamp });
            }
        }
    }

    /// Returns the current value, or `None` if the register is empty.
    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.entry.as_ref().map(|e| &e.value)
    }

    /// Returns the timestamp of the current entry, if any.
    #[must_use]
    pub fn timestamp(&self) -> Option<LamportTimestamp> {
        self.entry.as_ref().map(|e| e.timestamp)
    }
}

impl<T: Clone + PartialEq + Ord> Crdt for LwwRegister<T> {
    fn merge(&mut self, other: &Self) {
        match (&self.entry, &other.entry) {
            (_, None) => {}
            (None, Some(other_entry)) => {
                self.entry = Some(other_entry.clone());
            }
            (Some(mine), Some(theirs)) => {
                match theirs.timestamp.cmp(&mine.timestamp) {
                    Ordering::Greater => self.entry = Some(theirs.clone()),
                    Ordering::Equal if theirs.value != mine.value => {
                        // Deterministic tie-break when timestamps are identical.
                        if theirs.value > mine.value {
                            self.entry = Some(theirs.clone());
                        }
                    }
                    Ordering::Less | Ordering::Equal => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;

    #[test]
    fn new_register_is_empty() {
        let r: LwwRegister<String> = LwwRegister::new();
        assert_eq!(r.get(), None);
    }

    #[test]
    fn write_sets_value() {
        let mut clock = LamportClock::new(1);
        let mut r = LwwRegister::new();
        r.write("hello".to_string(), clock.tick());
        assert_eq!(r.get(), Some(&"hello".to_string()));
    }

    #[test]
    fn newer_write_overwrites_older() {
        let mut clock = LamportClock::new(1);
        let mut r = LwwRegister::new();
        r.write("first".to_string(), clock.tick());
        r.write("second".to_string(), clock.tick());
        assert_eq!(r.get(), Some(&"second".to_string()));
    }

    #[test]
    fn out_of_order_write_is_ignored() {
        // An older write arriving after a newer one should be ignored.
        let t1 = LamportTimestamp::new(1, 1);
        let t2 = LamportTimestamp::new(2, 1);
        let mut r = LwwRegister::new();
        r.write("newer".to_string(), t2);
        r.write("older".to_string(), t1);
        assert_eq!(r.get(), Some(&"newer".to_string()));
    }

    #[test]
    fn merge_keeps_highest_timestamp() {
        let t1 = LamportTimestamp::new(1, 1);
        let t2 = LamportTimestamp::new(5, 1);
        let mut a = LwwRegister::new();
        let mut b = LwwRegister::new();
        a.write("a".to_string(), t1);
        b.write("b".to_string(), t2);
        a.merge(&b);
        assert_eq!(a.get(), Some(&"b".to_string()));
    }

    #[test]
    fn merge_with_concurrent_writes_uses_replica_tiebreak() {
        // Same counter, different replicas: higher replica ID wins.
        let from_replica_1 = LamportTimestamp::new(1, 1);
        let from_replica_2 = LamportTimestamp::new(1, 2);

        let mut a = LwwRegister::new();
        let mut b = LwwRegister::new();
        a.write("red".to_string(), from_replica_1);
        b.write("blue".to_string(), from_replica_2);

        let mut a_then_b = a.clone();
        a_then_b.merge(&b);

        let mut b_then_a = b.clone();
        b_then_a.merge(&a);

        // Both must converge to the same winner regardless of merge order.
        assert_eq!(a_then_b.get(), b_then_a.get());
        assert_eq!(a_then_b.get(), Some(&"blue".to_string()));
    }
}
