//! Register-CRDT-er: Last-Writer-Wins Register.
//!
//! En *register* er en celle som holder én verdi om gangen. I et distribuert
//! system kan flere replikaer skrive til samme register samtidig; et
//! Last-Writer-Wins (LWW) Register håndterer dette ved å beholde den skrivingen
//! som har høyest tidsstempel, der "tid" er en Lamport-klokke for å garantere
//! konvergens uavhengig av fysiske klokker.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{clock::LamportTimestamp, Crdt};

/// Et Last-Writer-Wins register.
///
/// # Konsept
///
/// Tilstanden er enten tom, eller består av en `(verdi, tidsstempel)`. Når to
/// LWW-Registre merges, beholder vi den tilstanden som har høyest tidsstempel.
/// Fordi `LamportTimestamp` har en total ordning (counter, deretter replika-id),
/// vil alle replikaer alltid komme til samme konklusjon om hvem som vant.
///
/// # Informasjonstap
///
/// LWW-Register er den første CRDT-en vi har sett som *taper informasjon*:
/// hvis to replikaer skriver samtidig, blir den ene skrivingen kastet. Dette
/// er et bevisst designvalg som passer for verdier der det ikke gir mening å
/// "kombinere" (f.eks. en brukers nåværende navn), men ikke for verdier som
/// må akkumulere (f.eks. en handlekurv — bruk OR-Set i stedet).
///
/// # CRDT-egenskaper
///
/// Merge-operasjonen (ta tilstand med høyest tidsstempel) er kommutativ,
/// assosiativ og idempotent fordi max er det på en total ordning.
///
/// # Eksempel
///
/// ```
/// use crdt_lib::clock::LamportClock;
/// use crdt_lib::register::LwwRegister;
/// use crdt_lib::Crdt;
///
/// let mut clock_a = LamportClock::new(1);
/// let mut clock_b = LamportClock::new(2);
///
/// let mut a = LwwRegister::new();
/// let mut b = LwwRegister::new();
///
/// a.write("rød".to_string(), clock_a.tick());
/// b.write("blå".to_string(), clock_b.tick());
///
/// a.merge(&b);
/// // Begge har counter=1, men replika 2 vinner tie-break, så "blå" vinner.
/// assert_eq!(a.get(), Some(&"blå".to_string()));
/// ```
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
    /// Lager et tomt register.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Skriver en verdi med et gitt tidsstempel.
    ///
    /// Skrivingen blir bare permanent hvis tidsstempelet er høyere enn det
    /// nåværende. Ellers ignoreres skrivingen — dette er nødvendig for at
    /// `write` skal bevare CRDT-egenskapene under reordering.
    pub fn write(&mut self, value: T, timestamp: LamportTimestamp) {
        match &self.entry {
            Some(current) if current.timestamp >= timestamp => {
                // Ny skriving er ikke nyere; behold gammel verdi.
            }
            _ => {
                self.entry = Some(Entry { value, timestamp });
            }
        }
    }

    /// Returnerer den nåværende verdien, eller `None` hvis registeret er tomt.
    #[must_use]
    pub fn get(&self) -> Option<&T> {
        self.entry.as_ref().map(|e| &e.value)
    }

    /// Returnerer tidsstempelet til den nåværende skrivingen, om noen.
    #[must_use]
    pub fn timestamp(&self) -> Option<LamportTimestamp> {
        self.entry.as_ref().map(|e| e.timestamp)
    }
}

impl<T: Clone + PartialEq + Ord> Crdt for LwwRegister<T> {
    fn merge(&mut self, other: &Self) {
        match (&self.entry, &other.entry) {
            (_, None) => {
                // Ingenting å merge inn.
            }
            (None, Some(other_entry)) => {
                self.entry = Some(other_entry.clone());
            }
            (Some(mine), Some(theirs)) => {
                match theirs.timestamp.cmp(&mine.timestamp) {
                    Ordering::Greater => self.entry = Some(theirs.clone()),
                    Ordering::Equal if theirs.value != mine.value => {
                        // Deterministisk tie-break når tidsstempel er identisk.
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
        r.write("hei".to_string(), clock.tick());
        assert_eq!(r.get(), Some(&"hei".to_string()));
    }

    #[test]
    fn newer_write_overwrites_older() {
        let mut clock = LamportClock::new(1);
        let mut r = LwwRegister::new();
        r.write("første".to_string(), clock.tick());
        r.write("andre".to_string(), clock.tick());
        assert_eq!(r.get(), Some(&"andre".to_string()));
    }

    #[test]
    fn out_of_order_write_is_ignored() {
        // Hvis vi mottar en eldre skriving etter en nyere, skal den ignoreres.
        let t1 = LamportTimestamp::new(1, 1);
        let t2 = LamportTimestamp::new(2, 1);
        let mut r = LwwRegister::new();
        r.write("nyere".to_string(), t2);
        r.write("eldre".to_string(), t1);
        assert_eq!(r.get(), Some(&"nyere".to_string()));
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
        // Samme counter, ulik replika: høyere replika-id vinner.
        let from_replica_1 = LamportTimestamp::new(1, 1);
        let from_replica_2 = LamportTimestamp::new(1, 2);

        let mut a = LwwRegister::new();
        let mut b = LwwRegister::new();
        a.write("rød".to_string(), from_replica_1);
        b.write("blå".to_string(), from_replica_2);

        let mut a_then_b = a.clone();
        a_then_b.merge(&b);

        let mut b_then_a = b.clone();
        b_then_a.merge(&a);

        // Begge skal konvergere til samme vinner uansett merge-rekkefølge.
        assert_eq!(a_then_b.get(), b_then_a.get());
        assert_eq!(a_then_b.get(), Some(&"blå".to_string()));
    }
}
