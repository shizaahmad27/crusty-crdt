//! Tekst-CRDT-er: Replicated Growable Array (RGA).
//!
//! RGA er en sekvens-CRDT som tillater samtidig redigering av en delt streng
//! eller liste. Hvert tegn har en uforanderlig identitet (her: et
//! [`LamportTimestamp`]), og innsetting skjer "etter en gitt ID" snarere enn
//! "på en gitt indeks". Dette gjør at samtidige innsettinger fra forskjellige
//! replikaer kan flettes deterministisk.
//!
//! # Algoritme
//!
//! Hvert tegn lagres som en `Node` med:
//! - en unik `id` (vår Lamport-stempel),
//! - en `parent` som er IDen til tegnet det ble satt inn etter (`None` for det
//!   første tegnet),
//! - selve `value` (her: `char`),
//! - et `deleted`-flagg (tombstone).
//!
//! Den interne representasjonen er en `Vec<Node>` sortert etter RGA-regelen:
//!
//! 1. Et tegn kommer alltid etter sin forelder.
//! 2. Søsken (samme forelder) sorteres etter ID i *synkende* rekkefølge —
//!    nyere ID kommer først.
//!
//! Merge er union av nodemengden; etter merge sorteres listen på nytt.

use serde::{Deserialize, Serialize};

use crate::clock::LamportTimestamp;
use crate::Crdt;

/// Én node i RGA-strukturen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Node {
    id: LamportTimestamp,
    parent: Option<LamportTimestamp>,
    value: char,
    deleted: bool,
}

/// En Replicated Growable Array for tekst.
///
/// Støtter samtidig innsetting og sletting på tvers av replikaer med
/// deterministisk konvergens.
///
/// # Eksempel
///
/// ```
/// use crdt_lib::clock::LamportClock;
/// use crdt_lib::text::Rga;
/// use crdt_lib::Crdt;
///
/// let mut clock_a = LamportClock::new(1);
/// let mut clock_b = LamportClock::new(2);
///
/// // Begge starter fra tom tekst og setter inn 'A' og 'B' samtidig
/// let mut a = Rga::new();
/// let mut b = Rga::new();
///
/// a.insert_after(None, 'A', clock_a.tick());
/// b.insert_after(None, 'B', clock_b.tick());
///
/// a.merge(&b);
/// b.merge(&a);
///
/// // Begge konvergerer til samme streng (rekkefølge er deterministisk)
/// assert_eq!(a.to_string(), b.to_string());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rga {
    nodes: Vec<Node>,
}

impl Rga {
    /// Lager en tom RGA.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Setter inn `value` rett etter noden med ID `parent`. Hvis `parent` er
    /// `None`, settes tegnet inn først i listen.
    ///
    /// `id` må være globalt unik — typisk fra en lokal `LamportClock::tick`.
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

    /// Markerer noden med gitt ID som slettet (tombstone). Hvis IDen ikke
    /// finnes, er det en no-op.
    pub fn delete(&mut self, id: LamportTimestamp) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            node.deleted = true;
        }
    }

    /// Returnerer IDen til den synlige noden på posisjon `index` i den
    /// renderte teksten. Brukes for å konvertere "brukerens posisjon" til en
    /// stabil ID som kan settes inn etter.
    #[must_use]
    pub fn id_at_visible_index(&self, index: usize) -> Option<LamportTimestamp> {
        self.nodes
            .iter()
            .filter(|n| !n.deleted)
            .nth(index)
            .map(|n| n.id)
    }

    /// Returnerer antall synlige tegn (eksklusive tombstones).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| !n.deleted).count()
    }

    /// Returnerer `true` hvis det ikke finnes noen synlige tegn.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.iter().all(|n| n.deleted)
    }

    /// Renderer teksten ved å lese ut alle ikke-tombstonede noder i sortert
    /// rekkefølge.
    #[must_use]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.nodes
            .iter()
            .filter(|n| !n.deleted)
            .map(|n| n.value)
            .collect()
    }

    /// Sorterer den interne node-listen etter RGA-ordningen.
    ///
    /// Dette er kjernen i algoritmen. Vi bygger en topologisk-sortert
    /// rekkefølge der barn-noder kommer etter sin forelder, og søsken
    /// (samme forelder) sorteres etter ID i synkende rekkefølge.
    fn sort_nodes(&mut self) {
        // Bygg en map fra forelder-ID til mengden av barn (sortert).
        // Vi gjør dette ved en enkel rekursiv tilnærming: traversér fra roten,
        // og ved hver node, plukk ut barna sortert etter ID synkende.
        let original = std::mem::take(&mut self.nodes);

        // Indeksér nodene etter ID for raskt oppslag.
        let by_parent: std::collections::BTreeMap<
            Option<LamportTimestamp>,
            Vec<Node>,
        > = {
            let mut map: std::collections::BTreeMap<_, Vec<Node>> = std::collections::BTreeMap::new();
            for node in original {
                map.entry(node.parent).or_default().push(node);
            }
            // Sorter barn under hver forelder etter ID synkende.
            for children in map.values_mut() {
                children.sort_by(|a, b| b.id.cmp(&a.id));
            }
            map
        };

        let mut result = Vec::new();
        self.append_in_order(&by_parent, None, &mut result);
        self.nodes = result;
    }

    /// Rekursiv hjelper: traverserer "etter denne forelderen, så hennes barn,
    /// så deres barn", som er nøyaktig RGA-rekkefølgen.
    fn append_in_order(
        &self,
        by_parent: &std::collections::BTreeMap<Option<LamportTimestamp>, Vec<Node>>,
        parent: Option<LamportTimestamp>,
        out: &mut Vec<Node>,
    ) {
        if let Some(children) = by_parent.get(&parent) {
            for child in children {
                let child_id = child.id;
                out.push(child.clone());
                self.append_in_order(by_parent, Some(child_id), out);
            }
        }
    }
}

impl Crdt for Rga {
    fn merge(&mut self, other: &Self) {
        // Union: legg til alle noder fra `other` som vi ikke allerede har, og
        // for noder vi har, oppdater `deleted`-flagget til OR-en av begge.
        // Dette bevarer monotonisitet: en sletting kan ikke "angres".
        use std::collections::BTreeMap;

        let mut by_id: BTreeMap<LamportTimestamp, Node> = self
            .nodes
            .drain(..)
            .map(|n| (n.id, n))
            .collect();

        for node in &other.nodes {
            by_id
                .entry(node.id)
                .and_modify(|existing| {
                    existing.deleted = existing.deleted || node.deleted;
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
        // Begge replikaer setter inn et tegn etter samme forelder samtidig.
        // RGA-regelen sier: høyere ID vinner posisjonen først.
        let mut clock1 = LamportClock::new(1);
        let mut clock2 = LamportClock::new(2);

        // Felles utgangspunkt: tegnet 'H' med id fra replika 1.
        let h_id = LamportTimestamp::new(1, 1);

        let mut a = Rga::new();
        a.insert_after(None, 'H', h_id);
        let mut b = a.clone();

        // Replika 1 setter inn 'A' etter H
        clock1.observe(1);
        let a_id = clock1.tick();
        a.insert_after(Some(h_id), 'A', a_id);

        // Replika 2 setter inn 'B' etter H samtidig
        clock2.observe(1);
        let b_id = clock2.tick();
        b.insert_after(Some(h_id), 'B', b_id);

        a.merge(&b);
        b.merge(&a);

        // Begge skal nå ha samme streng (konvergens)
        assert_eq!(a.to_string(), b.to_string());
        // Lengden er 3 (H + de to nye tegnene)
        assert_eq!(a.len(), 3);
        // Førstebokstav skal være 'H'
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
        // Hvis én replika har slettet et tegn, skal mergen bevare slettingen.
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
        // Synlig tekst: "AC". Indeks 1 skal nå være C, ikke B.
        assert_eq!(r.id_at_visible_index(1), Some(c));
    }
}
