//! Text CRDTs: Replicated Growable Array (RGA).
//!
//! RGA is a sequence CRDT that supports concurrent editing of a shared string
//! or list. Each character has an immutable identity (a [`LamportTimestamp`]),
//! and insertion is expressed as "after a given ID" rather than "at a given
//! index". This lets concurrent insertions from different replicas be merged
//! deterministically.
//!
//! # Algorithm
//!
//! Each character is stored as a `Node` with:
//! - a unique `id` (a Lamport timestamp),
//! - a `parent` — the `id` of the character it was inserted after (`None` for
//!   the first character),
//! - the `value` (`char`),
//! - a `deleted` tombstone flag.
//!
//! The internal `Vec<Node>` is sorted by the RGA rule:
//! 1. A character always comes after its parent.
//! 2. Siblings (same parent) are sorted by ID in *descending* order — newer
//!    IDs come first.
//!
//! Merge is the union of the node sets, followed by re-sorting.

use serde::{Deserialize, Serialize};

use crate::clock::LamportTimestamp;
use crate::Crdt;

/// One node in the RGA structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Node {
    id: LamportTimestamp,
    parent: Option<LamportTimestamp>,
    value: char,
    deleted: bool,
}

/// A Replicated Growable Array for text.
///
/// Supports concurrent insertion and deletion across replicas with
/// deterministic convergence.
///
/// # Example
///
/// ```
/// use crdt_lib::clock::LamportClock;
/// use crdt_lib::text::Rga;
/// use crdt_lib::Crdt;
///
/// let mut clock_a = LamportClock::new(1);
/// let mut clock_b = LamportClock::new(2);
///
/// // Both replicas start from an empty document and insert concurrently.
/// let mut a = Rga::new();
/// let mut b = Rga::new();
///
/// a.insert_after(None, 'A', clock_a.tick());
/// b.insert_after(None, 'B', clock_b.tick());
///
/// a.merge(&b);
/// b.merge(&a);
///
/// // Both converge to the same string (order is deterministic).
/// assert_eq!(a.to_string(), b.to_string());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rga {
    nodes: Vec<Node>,
}

impl Rga {
    /// Creates an empty RGA.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `value` immediately after the node with the given `parent` ID.
    /// If `parent` is `None`, the character is inserted at the beginning.
    ///
    /// `id` must be globally unique — typically from a local `LamportClock::tick`.
    pub fn insert_after(
        &mut self,
        parent: Option<LamportTimestamp>,
        value: char,
        id: LamportTimestamp,
    ) {
        let node = Node {
            id,
            parent,
            value,
            deleted: false,
        };
        self.nodes.push(node);
        self.sort_nodes();
    }

    /// Marks the node with the given ID as deleted (tombstone). No-op if not found.
    pub fn delete(&mut self, id: LamportTimestamp) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            node.deleted = true;
        }
    }

    /// Returns the ID of the visible node at `index` in the rendered text.
    ///
    /// Use this to convert a user-facing position into a stable ID for insertion.
    #[must_use]
    pub fn id_at_visible_index(&self, index: usize) -> Option<LamportTimestamp> {
        self.nodes
            .iter()
            .filter(|n| !n.deleted)
            .nth(index)
            .map(|n| n.id)
    }

    /// Returns the number of visible characters (excluding tombstones).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| !n.deleted).count()
    }

    /// Returns `true` if there are no visible characters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.iter().all(|n| n.deleted)
    }

    /// Renders the text by collecting all non-tombstoned nodes in sorted order.
    #[must_use]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.nodes
            .iter()
            .filter(|n| !n.deleted)
            .map(|n| n.value)
            .collect()
    }

    /// Sorts the internal node list into RGA order.
    ///
    /// Builds a map from parent ID to children, sorts each sibling group by ID
    /// descending, then traverses recursively to produce a topologically sorted
    /// output where every node follows its parent.
    fn sort_nodes(&mut self) {
        let original = std::mem::take(&mut self.nodes);

        let by_parent: std::collections::BTreeMap<Option<LamportTimestamp>, Vec<Node>> = {
            let mut map: std::collections::BTreeMap<_, Vec<Node>> =
                std::collections::BTreeMap::new();
            for node in original {
                map.entry(node.parent).or_default().push(node);
            }
            // Sort siblings by ID descending so newer insertions appear first.
            for children in map.values_mut() {
                children.sort_by_key(|b| std::cmp::Reverse(b.id));
            }
            map
        };

        let mut result = Vec::new();
        Self::append_in_order(&by_parent, None, &mut result);
        self.nodes = result;
    }

    /// Recursively appends nodes in RGA order: each node followed by its children.
    fn append_in_order(
        by_parent: &std::collections::BTreeMap<Option<LamportTimestamp>, Vec<Node>>,
        parent: Option<LamportTimestamp>,
        out: &mut Vec<Node>,
    ) {
        if let Some(children) = by_parent.get(&parent) {
            for child in children {
                let child_id = child.id;
                out.push(child.clone());
                Self::append_in_order(by_parent, Some(child_id), out);
            }
        }
    }
}

impl Crdt for Rga {
    fn merge(&mut self, other: &Self) {
        // Union: add nodes from `other` we don't have, and for shared nodes OR
        // the deleted flags so deletions are never undone.
        use std::collections::BTreeMap;

        let mut by_id: BTreeMap<LamportTimestamp, Node> =
            self.nodes.drain(..).map(|n| (n.id, n)).collect();

        for node in &other.nodes {
            by_id
                .entry(node.id)
                .and_modify(|existing| {
                    existing.deleted = existing.deleted || node.deleted;
                    existing.value = existing.value.max(node.value);
                    existing.parent = std::cmp::max(existing.parent, node.parent);
                })
                .or_insert_with(|| node.clone());
        }

        self.nodes = by_id.into_values().collect();
        self.sort_nodes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;

    #[test]
    fn new_rga_is_empty() {
        let r = Rga::new();
        assert_eq!(r.to_string(), "");
        assert!(r.is_empty());
    }

    #[test]
    fn insert_at_root() {
        let mut clock = LamportClock::new(1);
        let mut r = Rga::new();
        r.insert_after(None, 'H', clock.tick());
        assert_eq!(r.to_string(), "H");
    }

    #[test]
    fn insert_sequentially() {
        let mut clock = LamportClock::new(1);
        let mut r = Rga::new();
        let h = clock.tick();
        r.insert_after(None, 'H', h);
        let e = clock.tick();
        r.insert_after(Some(h), 'E', e);
        let i = clock.tick();
        r.insert_after(Some(e), 'I', i);
        assert_eq!(r.to_string(), "HEI");
    }

    #[test]
    fn delete_creates_tombstone() {
        let mut clock = LamportClock::new(1);
        let mut r = Rga::new();
        let h = clock.tick();
        r.insert_after(None, 'H', h);
        let i = clock.tick();
        r.insert_after(Some(h), 'I', i);
        r.delete(h);
        assert_eq!(r.to_string(), "I");
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn concurrent_inserts_at_same_position_converge() {
        // Both replicas insert after the same parent; higher ID wins the position.
        let mut clock1 = LamportClock::new(1);
        let mut clock2 = LamportClock::new(2);

        let h_id = LamportTimestamp::new(1, 1);

        let mut a = Rga::new();
        a.insert_after(None, 'H', h_id);
        let mut b = a.clone();

        clock1.observe(1);
        let a_id = clock1.tick();
        a.insert_after(Some(h_id), 'A', a_id);

        clock2.observe(1);
        let b_id = clock2.tick();
        b.insert_after(Some(h_id), 'B', b_id);

        a.merge(&b);
        b.merge(&a);

        assert_eq!(a.to_string(), b.to_string());
        assert_eq!(a.len(), 3);
        assert!(a.to_string().starts_with('H'));
    }

    #[test]
    fn merge_is_idempotent() {
        let mut clock = LamportClock::new(1);
        let mut r = Rga::new();
        let h = clock.tick();
        r.insert_after(None, 'A', h);
        let i = clock.tick();
        r.insert_after(Some(h), 'B', i);

        let snapshot = r.clone();
        r.merge(&snapshot);
        assert_eq!(r, snapshot);
    }

    #[test]
    fn merge_preserves_deletes() {
        // A deletion on one replica must be preserved after merging.
        let mut clock = LamportClock::new(1);
        let h = clock.tick();

        let mut a = Rga::new();
        a.insert_after(None, 'X', h);
        let mut b = a.clone();
        a.delete(h);

        b.merge(&a);
        assert_eq!(b.to_string(), "");
    }

    #[test]
    fn id_at_visible_index_skips_tombstones() {
        let mut clock = LamportClock::new(1);
        let mut r = Rga::new();
        let a = clock.tick();
        r.insert_after(None, 'A', a);
        let b = clock.tick();
        r.insert_after(Some(a), 'B', b);
        let c = clock.tick();
        r.insert_after(Some(b), 'C', c);

        r.delete(b);
        // Visible text: "AC". Index 1 should now be C, not B.
        assert_eq!(r.id_at_visible_index(1), Some(c));
    }
}
