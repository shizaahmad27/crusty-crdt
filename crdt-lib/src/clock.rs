//! Logical clocks for CRDTs.
//!
//! Distributed systems cannot rely on physical clocks for event ordering: clocks
//! are never perfectly synchronized and can go backwards during NTP corrections.
//! Logical clocks solve this by defining "time" as a counter that grows with
//! causality rather than wall-clock time.
//!
//! This module implements the Lamport clock (Lamport, 1978).

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::ReplicaId;

/// A Lamport timestamp extended with `ReplicaId` for total ordering.
///
/// A plain Lamport clock gives only a *partial* order. To always break ties
/// deterministically, timestamps use `(counter, replica_id)` as a composite
/// key: the higher replica ID wins when counters are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LamportTimestamp {
    /// The logical clock value.
    pub counter: u64,
    /// The replica that generated this timestamp (used for tie-breaking).
    pub replica: ReplicaId,
}

impl LamportTimestamp {
    /// Creates a new timestamp.
    #[must_use]
    pub const fn new(counter: u64, replica: ReplicaId) -> Self {
        Self { counter, replica }
    }
}

// Order by counter first, then replica ID as a tie-breaker, giving a total order.
impl Ord for LamportTimestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.counter
            .cmp(&other.counter)
            .then_with(|| self.replica.cmp(&other.replica))
    }
}

impl PartialOrd for LamportTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A Lamport clock that generates monotonically increasing timestamps.
///
/// Rules:
/// 1. **Local event:** increment counter by 1.
/// 2. **Receive message with clock `t`:** set counter to `max(own, t) + 1`.
///

#[derive(Debug, Clone, Default)]
pub struct LamportClock {
    counter: u64,
    replica: ReplicaId,
}

impl LamportClock {
    /// Creates a new clock for the given replica, starting at 0.
    #[must_use]
    pub const fn new(replica: ReplicaId) -> Self {
        Self {
            counter: 0,
            replica,
        }
    }

    /// Generates a new timestamp for a local event, advancing the counter by 1.
    pub fn tick(&mut self) -> LamportTimestamp {
        self.counter = self.counter.saturating_add(1);
        LamportTimestamp::new(self.counter, self.replica)
    }

    /// Updates the clock after observing an incoming timestamp.
    ///
    /// Sets the counter to `max(own, observed) + 1`, ensuring any subsequent
    /// local event gets a timestamp strictly greater than both.
    pub fn observe(&mut self, observed: u64) -> LamportTimestamp {
        self.counter = self.counter.max(observed).saturating_add(1);
        LamportTimestamp::new(self.counter, self.replica)
    }

    /// Returns the current counter value without advancing it.
    #[must_use]
    pub const fn current(&self) -> u64 {
        self.counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_monotonic() {
        let mut c = LamportClock::new(1);
        let t1 = c.tick();
        let t2 = c.tick();
        assert!(t2 > t1);
    }

    #[test]
    fn observe_advances_beyond_seen_value() {
        let mut c = LamportClock::new(1);
        let t = c.observe(42);
        assert!(t.counter > 42);
    }

    #[test]
    fn observe_with_lower_value_still_advances() {
        let mut c = LamportClock::new(1);
        c.tick(); // counter = 1
        c.tick(); // counter = 2
        let t = c.observe(0); // counter = max(2, 0) + 1 = 3
        assert_eq!(t.counter, 3);
    }

    #[test]
    fn timestamps_have_total_order_via_replica_tiebreak() {
        let a = LamportTimestamp::new(5, 1);
        let b = LamportTimestamp::new(5, 2);
        assert!(b > a);
    }

    #[test]
    fn counter_dominates_replica_in_ordering() {
        let a = LamportTimestamp::new(10, 1); // higher counter
        let b = LamportTimestamp::new(5, 99); // higher replica, lower counter
        assert!(a > b);
    }
}
