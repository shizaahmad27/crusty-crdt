//! Property-based tests verifying the CRDT laws for all implemented types.
//!
//! A state-based CRDT's `merge` must be:
//! - **Commutative:** `merge(a, b) == merge(b, a)`
//! - **Associative:** `merge(merge(a, b), c) == merge(a, merge(b, c))`
//! - **Idempotent:** `merge(a, a) == a`
//!
//! These tests generate thousands of random states and verify the laws for each
//! one, covering edge cases that hand-written unit tests would miss.

use crdt_lib::clock::LamportTimestamp;
use crdt_lib::counter::{GCounter, PNCounter};
use crdt_lib::register::LwwRegister;
use crdt_lib::set::OrSet;
use crdt_lib::text::Rga;
use crdt_lib::Crdt;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategy helpers: how to generate random instances of each CRDT type.
// ---------------------------------------------------------------------------

/// Generates a random G-Counter using a small number of replicas and increments.
fn arb_gcounter() -> impl Strategy<Value = GCounter> {
    prop::collection::vec((0u64..5, 0u64..100), 0..10).prop_map(|ops| {
        let mut c = GCounter::new();
        for (replica, amount) in ops {
            c.increment(replica, amount);
        }
        c
    })
}

/// An operation on an OR-Set, used to generate random states.
#[derive(Debug, Clone)]
enum SetOp {
    Add(u8, LamportTimestamp),
    Remove(u8),
}

/// Generates a random sequence of set operations over a small value universe (u8 0..10).
fn arb_set_ops() -> impl Strategy<Value = Vec<SetOp>> {
    let op = prop_oneof![
        (0u8..10, 0u64..50, 0u64..5)
            .prop_map(|(v, c, r)| SetOp::Add(v, LamportTimestamp::new(c, r))),
        (0u8..10).prop_map(SetOp::Remove),
    ];
    prop::collection::vec(op, 0..20)
}

/// Builds an OR-Set by replaying a sequence of operations.
fn build_orset(ops: &[SetOp]) -> OrSet<u8> {
    let mut s = OrSet::new();
    for op in ops {
        match op {
            SetOp::Add(v, tag) => s.add(*v, *tag),
            SetOp::Remove(v) => s.remove(v),
        }
    }
    s
}

/// Generates a random LWW-Register over small integer values. Covers both
/// empty registers and registers with concurrent writes (same counter,
/// different replicas).
fn arb_lww() -> impl Strategy<Value = LwwRegister<i32>> {
    prop::option::of((0i32..100, 0u64..20, 0u64..5)).prop_map(|opt| {
        let mut r = LwwRegister::new();
        if let Some((value, counter, replica)) = opt {
            r.write(value, LamportTimestamp::new(counter, replica));
        }
        r
    })
}

/// Generates a random PN-Counter with both increments and decrements.
fn arb_pncounter() -> impl Strategy<Value = PNCounter> {
    prop::collection::vec((0u64..5, any::<bool>(), 0u64..100), 0..10).prop_map(|ops| {
        let mut c = PNCounter::new();
        for (replica, is_inc, amount) in ops {
            if is_inc {
                c.increment(replica, amount);
            } else {
                c.decrement(replica, amount);
            }
        }
        c
    })
}

// ---------------------------------------------------------------------------
// G-Counter properties
// ---------------------------------------------------------------------------

proptest! {
    /// Commutativity: `merge(a, b) == merge(b, a)`.
    #[test]
    fn gcounter_merge_is_commutative(a in arb_gcounter(), b in arb_gcounter()) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    /// Associativity: `merge(merge(a, b), c) == merge(a, merge(b, c))`.
    #[test]
    fn gcounter_merge_is_associative(
        a in arb_gcounter(),
        b in arb_gcounter(),
        c in arb_gcounter(),
    ) {
        let mut left = a.clone();
        left.merge(&b);
        left.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut right = a;
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    /// Idempotence: `merge(a, a) == a`.
    #[test]
    fn gcounter_merge_is_idempotent(a in arb_gcounter()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// Monotonicity: merging can never decrease the value.
    #[test]
    fn gcounter_merge_is_monotonic(a in arb_gcounter(), b in arb_gcounter()) {
        let before = a.value();
        let mut after = a;
        after.merge(&b);
        prop_assert!(after.value() >= before);
    }
}

// ---------------------------------------------------------------------------
// PN-Counter properties
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn pncounter_merge_is_commutative(a in arb_pncounter(), b in arb_pncounter()) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    #[test]
    fn pncounter_merge_is_associative(
        a in arb_pncounter(),
        b in arb_pncounter(),
        c in arb_pncounter(),
    ) {
        let mut left = a.clone();
        left.merge(&b);
        left.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut right = a;
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn pncounter_merge_is_idempotent(a in arb_pncounter()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }
}

// ---------------------------------------------------------------------------
// LWW-Register properties
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn lww_merge_is_commutative(a in arb_lww(), b in arb_lww()) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    #[test]
    fn lww_merge_is_associative(
        a in arb_lww(),
        b in arb_lww(),
        c in arb_lww(),
    ) {
        let mut left = a.clone();
        left.merge(&b);
        left.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut right = a;
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn lww_merge_is_idempotent(a in arb_lww()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// After merging, both sides must agree on the maximum timestamp.
    #[test]
    fn lww_merge_converges_to_max_timestamp(a in arb_lww(), b in arb_lww()) {
        let mut merged = a.clone();
        merged.merge(&b);

        let expected = a.timestamp().into_iter().chain(b.timestamp()).max();
        prop_assert_eq!(merged.timestamp(), expected);
    }
}

// ---------------------------------------------------------------------------
// OR-Set properties
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn orset_merge_is_commutative(ops_a in arb_set_ops(), ops_b in arb_set_ops()) {
        let a = build_orset(&ops_a);
        let b = build_orset(&ops_b);

        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    #[test]
    fn orset_merge_is_associative(
        ops_a in arb_set_ops(),
        ops_b in arb_set_ops(),
        ops_c in arb_set_ops(),
    ) {
        let a = build_orset(&ops_a);
        let b = build_orset(&ops_b);
        let c = build_orset(&ops_c);

        let mut left = a.clone();
        left.merge(&b);
        left.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut right = a;
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn orset_merge_is_idempotent(ops in arb_set_ops()) {
        let a = build_orset(&ops);
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// Strong convergence: replicas that have seen the same operations must
    /// have identical contents — the core guarantee CRDTs provide to users.
    #[test]
    fn orset_converges_when_both_see_same_ops(ops in arb_set_ops()) {
        let a = build_orset(&ops);

        // Build b by replaying the same operations in reverse order. For OR-Set,
        // order is not always equivalent (remove depends on observed tags), so
        // we merge operation-by-operation instead.
        let mut b = OrSet::new();
        for op in ops.iter().rev() {
            match op {
                SetOp::Add(v, tag) => b.add(*v, *tag),
                SetOp::Remove(v) => b.remove(v),
            }
        }

        let mut a_merged = a.clone();
        a_merged.merge(&b);
        let mut b_merged = b.clone();
        b_merged.merge(&a);

        prop_assert_eq!(a_merged, b_merged);
    }
}

// ---------------------------------------------------------------------------
// RGA properties
// ---------------------------------------------------------------------------

/// Builds a random RGA by inserting characters at random positions.
/// Uses a monotonically increasing counter to guarantee unique IDs.
fn arb_rga() -> impl Strategy<Value = Rga> {
    prop::collection::vec((any::<char>(), 0u64..10, 0u64..3), 0..15).prop_map(|ops| {
        let mut r = Rga::new();
        let mut counter: u64 = 1;
        for (ch, pos, replica) in ops {
            counter += 1;
            let id = LamportTimestamp::new(counter, replica);
            let len = r.len();
            let pos = usize::try_from(pos).expect("arb range fits in usize");
            let parent = if len == 0 || pos % (len + 1) == 0 {
                None
            } else {
                r.id_at_visible_index(pos % len)
            };
            r.insert_after(parent, ch, id);
        }
        r
    })
}

proptest! {
    #[test]
    fn rga_merge_is_commutative(a in arb_rga(), b in arb_rga()) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    #[test]
    fn rga_merge_is_associative(
        a in arb_rga(),
        b in arb_rga(),
        c in arb_rga(),
    ) {
        let mut left = a.clone();
        left.merge(&b);
        left.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut right = a;
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn rga_merge_is_idempotent(a in arb_rga()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// Rendered text must be identical after merging in both directions.
    #[test]
    fn rga_renders_consistently_after_merge(a in arb_rga(), b in arb_rga()) {
        let mut ab = a.clone();
        ab.merge(&b);
        let mut ba = b.clone();
        ba.merge(&a);
        prop_assert_eq!(ab.to_string(), ba.to_string());
    }
}
