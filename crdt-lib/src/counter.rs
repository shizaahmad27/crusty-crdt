//! Counter CRDTs: G-Counter and PN-Counter.
//!
//! Counters illustrate the core idea of state-based CRDTs: each replica holds
//! its own per-replica contribution that grows monotonically, and merge is a
//! join operation on a lattice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Crdt, ReplicaId};

/// A grow-only counter - a distributed counter that only supports increment.
///
/// # Concept
///
/// State is a map from `ReplicaId` to each replica's local contribution.
/// Each replica may only update its own entry. The total value is the sum of
/// all entries. Merge takes the per-replica maximum, so no value ever decreases.
///

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GCounter {
    /// Per-replica counts. `BTreeMap` gives deterministic iteration order,
    /// which helps with testing and serialization.
    counts: BTreeMap<ReplicaId, u64>,
}

impl GCounter {
    /// Creates an empty G-Counter with value 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments this replica's own count by `amount`.
    ///
    /// Each replica owns its own entry and must never modify another replica's
    /// value — this invariant guarantees monotonicity and convergence.
    pub fn increment(&mut self, replica: ReplicaId, amount: u64) {
        let entry = self.counts.entry(replica).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// Returns the total value as the sum of all replica contributions.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.counts.values().copied().sum()
    }

    /// Returns the local contribution of the given replica.
    #[must_use]
    pub fn value_for(&self, replica: ReplicaId) -> u64 {
        self.counts.get(&replica).copied().unwrap_or(0)
    }
}

impl Crdt for GCounter {
    fn merge(&mut self, other: &Self) {
        // Per-replica max: keep the highest observed count for each replica ID.
        for (&replica, &other_count) in &other.counts {
            let entry = self.counts.entry(replica).or_insert(0);
            *entry = (*entry).max(other_count);
        }
    }
}

/// A positive-negative counter — supports both increment and decrement.
///
/// # Concept
///
/// Decrement cannot be represented directly in a G-Counter without breaking
/// monotonicity. The trick is two G-Counters: one for increments (`p`) and one
/// for decrements (`n`). The total value is `p − n`. Both sub-counters still
/// grow monotonically, so all CRDT properties are preserved.
///
/// # Example
///
/// ```
/// use crdt_lib::counter::PNCounter;
/// use crdt_lib::Crdt;
///
/// let mut a = PNCounter::new();
/// a.increment(1, 10);
/// a.decrement(1, 3);
/// assert_eq!(a.value(), 7);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PNCounter {
    /// G-Counter tracking all increments per replica.
    positive: GCounter,
    /// G-Counter tracking all decrements per replica.
    negative: GCounter,
}

impl PNCounter {
    /// Creates an empty PN-Counter with value 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments this replica's count by `amount`.
    pub fn increment(&mut self, replica: ReplicaId, amount: u64) {
        self.positive.increment(replica, amount);
    }

    /// Decrements this replica's count by `amount`.
    pub fn decrement(&mut self, replica: ReplicaId, amount: u64) {
        self.negative.increment(replica, amount);
    }

    /// Returns the total value as `positive − negative`.
    ///
    /// Returns `i64` because the result may be negative.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn value(&self) -> i64 {
        self.positive.value() as i64 - self.negative.value() as i64
    }
}

impl Crdt for PNCounter {
    fn merge(&mut self, other: &Self) {
        self.positive.merge(&other.positive);
        self.negative.merge(&other.negative);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_counter_starts_at_zero() {
        let c = GCounter::new();
        assert_eq!(c.value(), 0);
    }

    #[test]
    fn g_counter_increments_locally() {
        let mut c = GCounter::new();
        c.increment(1, 5);
        c.increment(1, 3);
        assert_eq!(c.value(), 8);
    }

    #[test]
    fn g_counter_tracks_replicas_separately() {
        let mut c = GCounter::new();
        c.increment(1, 5);
        c.increment(2, 3);
        assert_eq!(c.value_for(1), 5);
        assert_eq!(c.value_for(2), 3);
        assert_eq!(c.value(), 8);
    }

    #[test]
    fn g_counter_merges_disjoint_replicas() {
        let mut a = GCounter::new();
        let mut b = GCounter::new();
        a.increment(1, 3);
        b.increment(2, 5);
        a.merge(&b);
        assert_eq!(a.value(), 8);
    }

    #[test]
    fn g_counter_merge_takes_max_per_replica() {
        let mut a = GCounter::new();
        let mut b = GCounter::new();
        a.increment(1, 3);
        b.increment(1, 7);
        a.merge(&b);
        assert_eq!(a.value(), 7);
    }

    #[test]
    fn g_counter_merge_is_idempotent() {
        let mut a = GCounter::new();
        a.increment(1, 5);
        let snapshot = a.clone();
        a.merge(&snapshot);
        assert_eq!(a, snapshot);
    }

    #[test]
    fn pn_counter_supports_decrement() {
        let mut c = PNCounter::new();
        c.increment(1, 10);
        c.decrement(1, 3);
        assert_eq!(c.value(), 7);
    }

    #[test]
    fn pn_counter_can_go_negative() {
        let mut c = PNCounter::new();
        c.decrement(1, 5);
        assert_eq!(c.value(), -5);
    }

    #[test]
    fn pn_counter_merges_correctly() {
        let mut a = PNCounter::new();
        let mut b = PNCounter::new();
        a.increment(1, 10);
        a.decrement(1, 2);
        b.increment(2, 5);
        b.decrement(2, 1);
        a.merge(&b);
        assert_eq!(a.value(), 12);
    }
}
