# crusty-crdt

[![CI](https://github.com/<brukernavn>/crusty-crdt/actions/workflows/ci.yml/badge.svg)](https://github.com/<brukernavn>/crusty-crdt/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-github--pages-blue)](https://<brukernavn>.github.io/crusty-crdt/)

> Et Rust-bibliotek for Conflict-free Replicated Data Types (CRDTs), implementert fra grunnen av som en bacheloroppgave i datakommunikasjon.

**Siste CI-kjøring:** [![CI](https://github.com/<brukernavn>/crusty-crdt/actions/workflows/ci.yml/badge.svg)](https://github.com/<brukernavn>/crusty-crdt/actions/workflows/ci.yml)
**API-dokumentasjon:** https://&lt;brukernavn&gt;.github.io/crusty-crdt/

## Introduksjon

Distribuerte applikasjoner som Google Docs, Figma og Apple Notes lar flere
brukere redigere samme data samtidig — også når noen av brukerne er offline.
Når endringene skal slås sammen oppstår det et fundamentalt problem: hvordan
løser man konflikter mellom samtidige endringer, deterministisk, uten en
sentral autoritet?

*Conflict-free Replicated Data Types* (CRDTs) er en familie datastrukturer som
løser dette ved konstruksjon. Hver node kan oppdatere sin lokale kopi uten
koordinering, og en matematisk garantert konvergens sørger for at alle noder
ender med samme tilstand når de utveksler oppdateringer.

Dette prosjektet implementerer et utvalg av de mest sentrale state-based
CRDTs (G-Counter, PN-Counter, LWW-Register, OR-Set, og RGA), samt en
peer-to-peer demoapplikasjon som viser hvordan biblioteket kan brukes i
praksis.

## Implementert funksjonalitet

- **G-Counter** — grow-only counter, det enkleste CRDT-eksempelet
- **PN-Counter** — counter som støtter både inkrement og dekrement
- **LWW-Register** — last-writer-wins register basert på Lamport-klokker
- **OR-Set** — observed-remove set som korrekt håndterer samtidig add/remove
- **RGA** — Replicated Growable Array for samtidig tekstredigering *(strekkmål)*
- **Peer-to-peer demo** med gossip-protokoll over TCP

Alle CRDT-typene er verifisert med property-based testing (`proptest`) for å
bevise at merge-operasjonen er kommutativ, assosiativ og idempotent.

## Fremtidig arbeid

- Delta-state CRDTs for å redusere nettverkstrafikk
- Garbage collection av tombstones i OR-Set
- Persistens (skrive tilstand til disk)
- Anti-entropi-protokoll med Merkle-trær
- Faktisk autentisering og kryptering i nettverkslaget
- Benchmarks med `criterion` for å sammenligne CRDT-implementasjonene

## Eksterne avhengigheter

| Avhengighet | Brukt til |
| --- | --- |
| `serde` | Serialisering av CRDT-tilstand til/fra et nøytralt format |
| `bincode` | Kompakt binær serialisering for nettverkstransport |
| `tokio` | Asynkron runtime for TCP-basert peer-to-peer-kommunikasjon |
| `tracing` + `tracing-subscriber` | Strukturert logging i demoapplikasjonen |
| `clap` | Kommandolinje-argumenter til demoapplikasjonen |
| `anyhow` | Ergonomisk feilhåndtering i demoapplikasjonen |
| `proptest` *(dev)* | Property-based testing av CRDT-lovene |
| `serde_json` *(dev)* | Lesbar serialisering i tester |

Ingen av avhengighetene implementerer CRDT-funksjonalitet — alle CRDT-er er
skrevet fra grunnen av i dette prosjektet.

## Installasjon

Forutsetninger: Rust 1.75 eller nyere ([installasjonsinstruksjoner](https://rustup.rs)).

```bash
git clone https://github.com/<brukernavn>/crusty-crdt.git
cd crusty-crdt
cargo build --release
```

## Bruk

### Biblioteket

```rust
use crdt_lib::counter::GCounter;
use crdt_lib::Crdt;

let mut a = GCounter::new();
let mut b = GCounter::new();

a.increment(1, 3);  // node 1 inkrementerer 3 ganger
b.increment(2, 5);  // node 2 inkrementerer 5 ganger

a.merge(&b);
assert_eq!(a.value(), 8);
```

### Demoapplikasjonen

Start tre noder i hver sin terminal:

```bash
cargo run --bin crdt-demo -- --id 1 --port 7001 --peers 127.0.0.1:7002,127.0.0.1:7003
cargo run --bin crdt-demo -- --id 2 --port 7002 --peers 127.0.0.1:7001,127.0.0.1:7003
cargo run --bin crdt-demo -- --id 3 --port 7003 --peers 127.0.0.1:7001,127.0.0.1:7002
```

## Tester

```bash
cargo test --workspace            # alle tester
cargo test -p crdt-lib            # bare biblioteket
cargo test --release properties   # bare property-tester (raskere)
```

## API-dokumentasjon

Generer og åpne lokalt:

```bash
cargo doc --workspace --no-deps --open
```

Eller se den publiserte versjonen: https://&lt;brukernavn&gt;.github.io/crusty-crdt/

## Eksterne kilder

Følgende eksterne kilder har vært sentrale i utformingen av løsningen og er
sitert i koden der det er relevant:

- Shapiro et al. (2011). *A comprehensive study of Convergent and Commutative
  Replicated Data Types*. INRIA Research Report 7506.
- Kleppmann (2020). *CRDTs: The Hard Parts*. Foredrag, Hydra Conference.
- Bartosz Sypytkowski. *An Introduction to State-based CRDTs*. Blogg-serie.

Ingen kode er kopiert fra disse kildene; algoritmene er implementert på
nytt basert på publiserte beskrivelser.

## Lisens

MIT
