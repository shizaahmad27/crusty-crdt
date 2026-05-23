//! Counter-CRDT-er: G-Counter og PN-Counter.
//!
<<<<<<< HEAD
//! Counters er det enkleste pedagogiske eksempelet på en CRDT, og illustrerer
//! kjerneideen bak state-based CRDTs: hver replika holder et *per-replika*
//! bidrag som vokser monotont, og merge er en *join*-operasjon i en lattice.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Crdt, ReplicaId};

/// En *grow-only counter* — en distribuert teller som bare kan inkrementere.
///
/// # Konsept
///
/// Tilstanden er en map fra `ReplicaId` til hver replikas lokale bidrag.
/// Hver replika får kun lov til å oppdatere *sin egen* oppføring. Den totale
/// verdien er summen av alle oppføringene.
///
/// Når to G-Countere merges, tar vi elementvis maksimum: for hver replika-id
/// beholder vi den høyeste verdien som er observert. Fordi hver replika bare
/// øker sin egen teller, kan ingen verdi minke, og elementvis max gir korrekt
/// konvergens uten å dobbelttelle.
///
/// # CRDT-egenskaper
///
/// `merge` er:
/// - **Kommutativ:** `max(a, b) = max(b, a)` per komponent
/// - **Assosiativ:** `max(max(a, b), c) = max(a, max(b, c))` per komponent
/// - **Idempotent:** `max(a, a) = a` per komponent
///
/// Disse egenskapene bevises maskinmessig av property-tester i
/// `tests/properties.rs`.
///
/// # Eksempel
///
/// ```
/// use crdt_lib::counter::GCounter;
/// use crdt_lib::Crdt;
///
/// let mut a = GCounter::new();
/// let mut b = GCounter::new();
///
/// a.increment(1, 3);
/// b.increment(2, 5);
///
/// a.merge(&b);
/// assert_eq!(a.value(), 8);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GCounter {
    /// Per-replika telling. `BTreeMap` gir deterministisk iterasjonsrekkefølge,
    /// noe som er nyttig for testing og serialisering.
    counts: BTreeMap<ReplicaId, u64>,
}

impl GCounter {
    /// Lager en tom G-Counter med verdi 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inkrementerer denne replikaens egen telling med `amount`.
    ///
    /// Hver replika eier sin egen oppføring; det er en sentral invariant at en
    /// replika aldri endrer en annen replikas verdi. Dette er det som garanterer
    /// monotonisitet og dermed konvergens.
    pub fn increment(&mut self, replica: ReplicaId, amount: u64) {
        let entry = self.counts.entry(replica).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    /// Returnerer den totale verdien som summen av alle replikaers bidrag.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.counts.values().copied().sum()
    }

    /// Returnerer det lokale bidraget til en gitt replika.
    #[must_use]
    pub fn value_for(&self, replica: ReplicaId) -> u64 {
        self.counts.get(&replica).copied().unwrap_or(0)
    }
}

impl Crdt for GCounter {
    fn merge(&mut self, other: &Self) {
        // Elementvis maksimum: for hver replika-id beholder vi den høyeste
        // observerte verdien. Replikaer som finnes i `other` men ikke i `self`
        // legges til; replikaer som finnes i begge oppdateres til max.
        for (&replica, &other_count) in &other.counts {
            let entry = self.counts.entry(replica).or_insert(0);
            *entry = (*entry).max(other_count);
        }
    }
}

/// En *positive-negative counter* — en distribuert teller som støtter både
/// inkrement og dekrement.
///
/// # Konsept
///
/// Et dekrement kan ikke representeres direkte i en G-Counter (det ville bryte
/// monotonisitet). Trikset er å bruke *to* G-Countere: én for inkrementer (`p`)
/// og én for dekrementer (`n`). Den totale verdien er differansen `p − n`.
///
/// Begge underliggende countere vokser fortsatt monotont, så CRDT-egenskapene
/// bevares.
///
/// # Eksempel
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
    positive: GCounter,
    negative: GCounter,
}

impl PNCounter {
    /// Lager en tom PN-Counter med verdi 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inkrementerer denne replikaens telling med `amount`.
    pub fn increment(&mut self, replica: ReplicaId, amount: u64) {
        self.positive.increment(replica, amount);
    }

    /// Dekrementerer denne replikaens telling med `amount`.
    pub fn decrement(&mut self, replica: ReplicaId, amount: u64) {
        self.negative.increment(replica, amount);
    }

    /// Returnerer den totale verdien som `positive − negative`.
    ///
    /// Returnerer en `i64` fordi resultatet kan være negativt.
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
        // To noder har sett ulike antall inkrementer fra samme replika.
        // Etter merge skal vi se den høyeste observerte verdien.
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
        // a: +10−2 = 8, b: +5−1 = 4, totalt: 12
        assert_eq!(a.value(), 12);
    }
}
=======
//! Implementeres i dag 2-3.

#![allow(missing_docs)]
>>>>>>> 8e6760f35255d085662623707ec51acbf0e1d572
