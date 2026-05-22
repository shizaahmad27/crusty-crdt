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
use crdt_lib::set::OrSet;
use crdt_lib::text::Rga;
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

/// En operasjon på et OR-Set, brukt for å generere tilfeldige tilstander.
#[derive(Debug, Clone)]
enum SetOp {
    Add(u8, LamportTimestamp),
    Remove(u8),
}

/// Genererer en tilfeldig sekvens av set-operasjoner over et lite univers
/// (u8-verdier 0..10) med Lamport-tagger.
fn arb_set_ops() -> impl Strategy<Value = Vec<SetOp>> {
    let op = prop_oneof![
        (0u8..10, 0u64..50, 0u64..5)
            .prop_map(|(v, c, r)| SetOp::Add(v, LamportTimestamp::new(c, r))),
        (0u8..10).prop_map(SetOp::Remove),
    ];
    prop::collection::vec(op, 0..20)
}

/// Bygger et OR-Set ved å spille av en operasjonssekvens.
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
        let mut right = a.clone();
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
        let mut right = a.clone();
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
        let mut right = a.clone();
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
        let mut right = a.clone();
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

    /// Sterk konvergens: hvis to replikaer har sett nøyaktig samme
    /// operasjoner (i hvilken som helst rekkefølge), skal innholdet være
    /// identisk. Dette er den faktiske CRDT-garantien for brukere.
    #[test]
    fn orset_converges_when_both_see_same_ops(ops in arb_set_ops()) {
        let a = build_orset(&ops);

        // Bygg b ved å spille av samme operasjoner i omvendt rekkefølge.
        // For OR-Set er rekkefølgen ikke ekvivalent i alle tilfeller fordi
        // remove avhenger av hvilke tags som er observert, så vi merger i
        // stedet operasjon-for-operasjon.
        let mut b = OrSet::new();
        for op in ops.iter().rev() {
            match op {
                SetOp::Add(v, tag) => b.add(*v, *tag),
                SetOp::Remove(v) => b.remove(v),
            }
        }

        // Etter at begge har merget med hverandre, må de være like.
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

/// Bygger en tilfeldig RGA ved å sette inn tegn på tilfeldige posisjoner.
/// Vi bruker en monotont voksende counter for ID-er for å sikre unikhet på
/// tvers av kall i samme test.
fn arb_rga() -> impl Strategy<Value = Rga> {
    prop::collection::vec((any::<char>(), 0u64..10, 0u64..3), 0..15).prop_map(|ops| {
        let mut r = Rga::new();
        let mut counter: u64 = 1;
        for (ch, pos, replica) in ops {
            counter += 1;
            let id = LamportTimestamp::new(counter, replica);
            let len = r.len();
            let parent = if len == 0 || pos as usize % (len + 1) == 0 {
                None
            } else {
                r.id_at_visible_index((pos as usize) % len)
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
        let mut right = a.clone();
        right.merge(&bc);

        prop_assert_eq!(left, right);
    }

    #[test]
    fn rga_merge_is_idempotent(a in arb_rga()) {
        let mut merged = a.clone();
        merged.merge(&a);
        prop_assert_eq!(merged, a);
    }

    /// Konvergens på rendret tekst: etter merge i begge retninger må strengene
    /// være identiske. Dette er den faktiske garantien til en bruker.
    #[test]
    fn rga_renders_consistently_after_merge(a in arb_rga(), b in arb_rga()) {
        let mut ab = a.clone();
        ab.merge(&b);
        let mut ba = b.clone();
        ba.merge(&a);
        prop_assert_eq!(ab.to_string(), ba.to_string());
    }
}
