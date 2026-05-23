//! Logiske klokker for CRDT-er.
//!
//! Distribuerte systemer kan ikke stole på fysiske klokker for å avgjøre
//! rekkefølge mellom hendelser: klokker er aldri perfekt synkronisert, og de
//! kan gå baklengs ved NTP-korreksjoner. *Logiske klokker* løser dette ved å
//! definere "tid" som et tall som vokser med kausalitet, ikke med
//! veggklokke-tid.
//!
//! Denne modulen implementerer Lamport-klokken (Lamport, 1978), den enkleste
//! formen for logisk klokke.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::ReplicaId;

/// Et Lamport-tidsstempel, utvidet med `ReplicaId` for total ordning.
///
/// # Konsept
///
/// En ren Lamport-klokke gir kun en *partiell* ordning på hendelser:
/// `clock(a) < clock(b)` betyr ikke nødvendigvis at `a` skjedde før `b` — de
/// kan ha vært samtidige (concurrent). For LWW-Register trenger vi imidlertid
/// en total ordning slik at vi alltid kan utpeke en vinner. Dette løses ved å
/// bruke `(klokke, replika-id)` som en sammensatt nøkkel: ved likt klokkeslett
/// vinner den høyeste replika-IDen.
///
/// Dette er deterministisk og konsistent på tvers av noder, men det er
/// vilkårlig hvilken replika som vinner. Det er en akseptabel pris for
/// konvergens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LamportTimestamp {
    /// Den logiske klokkeverdien.
    pub counter: u64,
    /// Replikaen som genererte tidsstempelet (brukt til tie-breaking).
    pub replica: ReplicaId,
}

impl LamportTimestamp {
    /// Lager et nytt tidsstempel.
    #[must_use]
    pub const fn new(counter: u64, replica: ReplicaId) -> Self {
        Self { counter, replica }
    }
}

// Sammenligning: først på counter, deretter på replika-id som tie-breaker.
// Dette gir en *total* ordning på alle tidsstempler på tvers av noder.
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

/// En Lamport-klokke som genererer monotont stigende tidsstempler.
///
/// Reglene er:
///
/// 1. **Lokal hendelse:** øk klokken med 1.
/// 2. **Motta melding med klokke `t`:** sett klokken til `max(egen, t) + 1`.
///
/// Disse to reglene garanterer at hvis hendelse `a` *kausalt påvirker*
/// hendelse `b`, så er `clock(a) < clock(b)`. Det motsatte gjelder ikke:
/// samtidige hendelser kan ha vilkårlig forhold mellom klokkeverdiene sine.
///
/// # Eksempel
///
/// ```
/// use crdt_lib::clock::LamportClock;
///
/// let mut clock = LamportClock::new(1);
/// let t1 = clock.tick();
/// let t2 = clock.tick();
/// assert!(t2 > t1);
///
/// // Mottar en melding med høyere klokke
/// let observed = clock.observe(100);
/// assert!(observed.counter > 100);
/// ```
#[derive(Debug, Clone, Default)]
pub struct LamportClock {
    counter: u64,
    replica: ReplicaId,
}

impl LamportClock {
    /// Lager en ny klokke for en gitt replika, startet på 0.
    #[must_use]
    pub const fn new(replica: ReplicaId) -> Self {
        Self {
            counter: 0,
            replica,
        }
    }

    /// Genererer et nytt tidsstempel for en lokal hendelse.
    ///
    /// Øker klokken med 1 og returnerer den nye verdien sammen med
    /// replika-IDen.
    pub fn tick(&mut self) -> LamportTimestamp {
        self.counter = self.counter.saturating_add(1);
        LamportTimestamp::new(self.counter, self.replica)
    }

    /// Observerer en innkommende klokkeverdi og oppdaterer egen klokke.
    ///
    /// Setter klokken til `max(egen, mottatt) + 1`. Dette garanterer at en
    /// hendelse som skjer etter mottak av meldingen får et tidsstempel som er
    /// høyere enn både egen tidligere klokke og avsenderens klokke.
    pub fn observe(&mut self, observed: u64) -> LamportTimestamp {
        self.counter = self.counter.max(observed).saturating_add(1);
        LamportTimestamp::new(self.counter, self.replica)
    }

    /// Returnerer den nåværende klokkeverdien uten å øke den.
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
        let t = c.observe(0); // counter blir max(2, 0) + 1 = 3
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
        let a = LamportTimestamp::new(10, 1); // høyere counter
        let b = LamportTimestamp::new(5, 99); // høyere replika, men lavere counter
        assert!(a > b);
    }
}
