//! Set-CRDT-er: Observed-Remove Set (OR-Set).
//!
//! En distribuert mengde er overraskende vanskelig fordi samtidige `add` og
//! `remove` av samme element er semantisk i konflikt: skal elementet være med
//! eller ikke? OR-Set løser dette ved å la hver `add` opprette en unik *tag*
//! som identifiserer akkurat den tilføyelsen, og bare la `remove` fjerne tags
//! den faktisk har observert. Dette gir "add wins" semantikk: en samtidig add
//! overlever en remove.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::{clock::LamportTimestamp, Crdt};

/// Et Observed-Remove Set.
///
/// # Konsept
///
/// Hver tilføyelse av et element `x` får en unik tag (her: et
/// `LamportTimestamp`). Settet holder en map fra elementer til sine tagger, og
/// en mengde av "tombstones" — tagger som har blitt fjernet. Et element
/// regnes som med i settet hvis det har minst én tag som *ikke* er en
/// tombstone.
///
/// `remove(x)` flytter alle nåværende tags for `x` til tombstones. Hvis en
/// annen replika samtidig har lagt til `x` med en ny tag som denne replikaen
/// ikke har sett, vil ikke remove kunne røre den taggen — og elementet vil
/// overleve mergen. Dette er **add-wins** semantikken.
///
/// # CRDT-egenskaper
///
/// `merge` er union av `tags` og union av `tombstones`. Union på mengder er
/// kommutativ, assosiativ og idempotent, så CRDT-lovene holder.
///
/// # Eksempel
///
/// ```
/// use crdt_lib::clock::LamportClock;
/// use crdt_lib::set::OrSet;
/// use crdt_lib::Crdt;
///
/// let mut clock = LamportClock::new(1);
/// let mut s = OrSet::new();
/// s.add("melk", clock.tick());
/// s.add("brød", clock.tick());
/// assert!(s.contains(&"melk"));
///
/// s.remove(&"melk");
/// assert!(!s.contains(&"melk"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrSet<T>
where
    T: Ord + Clone,
{
    /// Per-element mengde av tagger som har blitt lagt til.
    tags: BTreeMap<T, BTreeSet<LamportTimestamp>>,
    /// Mengde av tagger som har blitt fjernet (tombstones).
    tombstones: BTreeSet<LamportTimestamp>,
}

impl<T> Default for OrSet<T>
where
    T: Ord + Clone,
{
    fn default() -> Self {
        Self {
            tags: BTreeMap::new(),
            tombstones: BTreeSet::new(),
        }
    }
}

impl<T> OrSet<T>
where
    T: Ord + Clone + Hash,
{
    /// Lager et tomt OR-Set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Legger til `value` med en gitt unik tag.
    ///
    /// Taggen må være globalt unik (typisk fra en `LamportClock`). Hvis
    /// samme tag brukes for to forskjellige add-er, vil oppførselen være
    /// udefinert i forhold til konvergens.
    pub fn add(&mut self, value: T, tag: LamportTimestamp) {
        self.tags.entry(value).or_default().insert(tag);
    }

    /// Fjerner `value` ved å markere alle dets nåværende observerte tagger
    /// som tombstones.
    ///
    /// Dette er det "observed" i "observed-remove": vi kan kun fjerne tagger
    /// vi faktisk har sett. Tagger som blir lagt til av andre replikaer
    /// senere (eller samtidig, men ikke synlige ennå) vil overleve fjerningen.
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.tags.get(value) {
            for tag in tags {
                self.tombstones.insert(*tag);
            }
        }
    }

    /// Returnerer `true` hvis `value` har minst én tag som ikke er en
    /// tombstone — dvs. er fortsatt i settet.
    #[must_use]
    pub fn contains(&self, value: &T) -> bool {
        self.tags
            .get(value)
            .is_some_and(|tags| tags.iter().any(|tag| !self.tombstones.contains(tag)))
    }

    /// Returnerer en iterator over alle elementer som er med i settet.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.tags
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.tombstones.contains(tag)))
            .map(|(value, _)| value)
    }

    /// Returnerer antall elementer i settet.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Returnerer `true` hvis settet er tomt.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl<T> Crdt for OrSet<T>
where
    T: Ord + Clone + Hash,
{
    fn merge(&mut self, other: &Self) {
        // Union av tags per element.
        for (value, other_tags) in &other.tags {
            self.tags
                .entry(value.clone())
                .or_default()
                .extend(other_tags.iter().copied());
        }
        // Union av tombstones.
        self.tombstones.extend(other.tombstones.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::LamportClock;

    #[test]
    fn new_set_is_empty() {
        let s: OrSet<&str> = OrSet::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn add_makes_element_present() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        assert!(s.contains(&"a"));
    }

    #[test]
    fn remove_makes_element_absent() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.remove(&"a");
        assert!(!s.contains(&"a"));
    }

    #[test]
    fn re_add_after_remove_works() {
        // Etter remove kan elementet legges til igjen med en ny tag.
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.remove(&"a");
        s.add("a", clock.tick()); // ny tag, ikke tombstoned
        assert!(s.contains(&"a"));
    }

    #[test]
    fn concurrent_add_wins_over_remove() {
        // Dette er kjernesemantikken til OR-Set.
        // Replika 1 legger til, deretter fjerner.
        let mut clock1 = LamportClock::new(1);
        let mut a = OrSet::new();
        let tag1 = clock1.tick();
        a.add("x", tag1);
        a.remove(&"x");

        // Replika 2 starter fra samme initialtilstand (med tag1), men legger
        // til en ny instans samtidig — uten å se replika 1 sin remove.
        let mut clock2 = LamportClock::new(2);
        let mut b = OrSet::new();
        b.add("x", tag1); // har sett den opprinnelige tilføyelsen
        b.add("x", clock2.tick()); // ny samtidig tilføyelse

        // Merge: a sin remove fjerner kun tag1; b sin nye tag overlever.
        a.merge(&b);
        assert!(a.contains(&"x"));
    }

    #[test]
    fn merge_is_symmetric_for_concurrent_add_remove() {
        // Samme scenario som over, men sjekker at merge-rekkefølge ikke har
        // betydning — kommutativitet er CRDT-kjernen.
        let mut clock1 = LamportClock::new(1);
        let mut clock2 = LamportClock::new(2);

        let tag1 = clock1.tick();

        let mut a = OrSet::new();
        a.add("x", tag1);
        a.remove(&"x");

        let mut b = OrSet::new();
        b.add("x", tag1);
        b.add("x", clock2.tick());

        let mut a_then_b = a.clone();
        a_then_b.merge(&b);

        let mut b_then_a = b.clone();
        b_then_a.merge(&a);

        assert_eq!(a_then_b, b_then_a);
    }

    #[test]
    fn iter_yields_present_elements() {
        let mut clock = LamportClock::new(1);
        let mut s = OrSet::new();
        s.add("a", clock.tick());
        s.add("b", clock.tick());
        s.add("c", clock.tick());
        s.remove(&"b");

        let mut present: Vec<&&str> = s.iter().collect();
        present.sort();
        assert_eq!(present, vec![&"a", &"c"]);
    }
}
