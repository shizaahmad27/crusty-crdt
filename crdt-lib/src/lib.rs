//! # crdt-lib
//!
//! Et bibliotek for Conflict-free Replicated Data Types (CRDTs) implementert
//! fra grunnen av i Rust. Biblioteket er laget som en bacheloroppgave og fokuserer
//! på pedagogisk klarhet og matematisk korrekthet fremfor maksimal ytelse.
//!
//! ## Hva er en CRDT?
//!
//! En CRDT er en datastruktur som kan repliseres på tvers av flere noder,
//! oppdateres uavhengig og parallelt på hver node uten koordinering mellom dem,
//! og som garantert vil konvergere til samme tilstand på alle noder så lenge
//! de samme oppdateringene til slutt blir levert.
//!
//! ## Familier
//!
//! Dette biblioteket implementerer *state-based* (`CvRDT`) varianter. Disse er
//! definert av en `merge`-operasjon som må være:
//!
//! - **Kommutativ:** `merge(a, b) = merge(b, a)`
//! - **Assosiativ:** `merge(merge(a, b), c) = merge(a, merge(b, c))`
//! - **Idempotent:** `merge(a, a) = a`
//!
//! Disse tre egenskapene gjør at tilstanden danner en *join-semilattice*, og at
//! konvergens er garantert uansett rekkefølge eller duplisering av meldinger.
//!
//! ## Typer som er implementert
//!
//! - [`counter::GCounter`] — grow-only counter
//! - [`counter::PNCounter`] — counter som støtter inkrement og dekrement
//!
//! Kommer etter hvert: `LwwRegister`, `OrSet`, og `Rga`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod clock;
pub mod counter;
pub mod register;
pub mod set;
pub mod text;

/// Grensesnitt som alle state-based CRDTs i dette biblioteket implementerer.
///
/// Implementasjoner må garantere at `merge` er kommutativ, assosiativ og
/// idempotent. Disse egenskapene verifiseres med property-based tester.
pub trait Crdt {
    /// Slår sammen `other` inn i `self`. Etter dette skal `self` representere
    /// minste øvre grense (join) av de to tilstandene i lattisen.
    fn merge(&mut self, other: &Self);
}

/// Unik identifikator for en node i et CRDT-system.
///
/// Hver node må ha en globalt unik `ReplicaId`. I dette biblioteket bruker vi
/// en enkel `u64`; i et produksjonssystem ville man typisk brukt UUID.
pub type ReplicaId = u64;
