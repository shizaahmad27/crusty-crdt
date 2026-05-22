//! Property-based tester som verifiserer CRDT-lovene for alle implementerte typer.
//!
//! En state-based CRDT må ha en `merge`-operasjon som er:
//! - **Kommutativ:** `merge(a, b) == merge(b, a)`
//! - **Assosiativ:** `merge(merge(a, b), c) == merge(a, merge(b, c))`
//! - **Idempotent:** `merge(a, a) == a`
//!
//! Disse testene genererer tusenvis av tilfeldige tilstander og verifiserer at
//! lovene holder for hver eneste én. Dette er sterkere enn håndskrevne
//! enhetstester fordi det utforsker corner cases vi ikke ville tenkt på.

use crdt_lib::clock::LamportTimestamp;
use crdt_lib::counter::{GCounter, PNCounter};
use crdt_lib::register::LwwRegister;
use crdt_lib::Crdt;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategi-helpere: hvordan generere tilfeldige instanser av hver CRDT-type.
// ---------------------------------------------------------------------------

/// Genererer en tilfeldig G-Counter ved å bruke et lite antall replikaer og
/// inkrementer. Små verdier holder testene raske.
fn arb_gcounter() -> impl Strategy<Value = GCounter> {
    prop::collection::vec((0u64..5, 0u64..100), 0..10).prop_map(|ops| {
        let mut c = GCounter::new();
        for (replica, amount) in ops {
            c.increment(replica, amount);
        }
        c
    })
}

/// Genererer et tilfeldig LWW-Register over små heltallsverdier. Vi bruker
/// `i32` som verditype og lar tidsstempelet variere fritt; det viktigste er at
/// strategien dekker både tomme registre og registre med samtidige skrivinger
/// (samme counter, ulik replika).
fn arb_lww() -> impl Strategy<Value = LwwRegister<i32>> {
    prop::option::of((0i32..100, 0u64..20, 0u64..5)).prop_map(|opt| {
        let mut r = LwwRegister::new();
        if let Some((value, counter, replica)) = opt {
            r.write(value, LamportTimestamp::new(counter, replica));
        }
        r
    })
}

/// Genererer en tilfeldig PN-Counter med både inkrementer og dekrementer.
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
    /// Kommutativitet: `merge(a, b) == merge(b, a)`.
    #[test]
    fn gcounter_merge_is_commutative(a in arb_gcounter(), b in arb_gcounter()) {
        let mut ab = a.clone();
        ab.merge(&b);

        let mut ba = b.clone();
        ba.merge(&a);

        prop_assert_eq!(ab, ba);
    }

    /// Assosiativitet: `merge(merge(a, b), c) == merge(a, merge(b, c))`.
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

    /// Idempotens: `merge(a, a) == a`.
    #[test]
    fn gcounter_merge_is_idempotent(a in arb_gcounter()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// Verdien skal vokse monotont under merge: merging kan aldri minke verdien.
    /// Dette er en avledet konsekvens av at G-Counter danner en lattice.
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

    /// Konsekvens av total ordning: etter merge skal begge sider ha det samme
    /// tidsstempelet (eller begge være tomme).
    #[test]
    fn lww_merge_converges_to_max_timestamp(a in arb_lww(), b in arb_lww()) {
        let mut merged = a.clone();
        merged.merge(&b);

        let expected = a.timestamp().into_iter().chain(b.timestamp()).max();
        prop_assert_eq!(merged.timestamp(), expected);
    }
}
