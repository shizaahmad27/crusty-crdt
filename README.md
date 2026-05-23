# crusty-crdt

[![CI](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml/badge.svg)](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-rustdoc-blue)](https://shizaahmad27.github.io/crusty-crdt/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> Et Rust-bibliotek for Conflict-free Replicated Data Types (CRDTs), implementert
> fra grunnen av som prosjektoppgave i datakommunikasjon. Inkluderer fem
> CRDT-typer og en peer-to-peer demoapplikasjon.

**Siste CI-kjøring:** [GitHub Actions](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml) (badge over viser status for siste kjøring på `main`)

**API-dokumentasjon:** [shizaahmad27.github.io/crusty-crdt](https://shizaahmad27.github.io/crusty-crdt/)

---

## Introduksjon

Distribuerte applikasjoner som Google Docs, Figma og Apple Notes lar flere brukere redigere samme data samtidig, også når noen av brukerne er offline. Når endringene skal slås sammen, oppstår et fundamentalt problem: hvordan løse konflikter mellom samtidige endringer, deterministisk, uten en sentral autoritet?

*Conflict-free Replicated Data Types* (CRDTs) er en familie datastrukturer som løser dette ved konstruksjon. Hver node kan oppdatere sin lokale kopi uten koordinering, og en matematisk garantert konvergens sørger for at alle noder ender med samme tilstand når de utveksler oppdateringer. Garantien hviler på at `merge`-operasjonen er kommutativ, assosiativ og idempotent. Det er  egenskaper som tilsammen gjør tilstandsrommet til en *join-semilattice*.

Dette prosjektet implementerer fem state-based CRDTs fra grunnen av i Rust, verifiserer hver med property-based testing, og demonstrerer dem i en peer-to-peer applikasjon med gossip-protokoll over TCP. Alle CRDT-egenskapene er bevist matematisk gjennom ~4900 automatisk genererte testtilfeller per testkjøring.

---

## Implementert funksjonalitet

### CRDT-typer


| Type             | Modul                      | Beskrivelse                                                          |
| ---------------- | -------------------------- | -------------------------------------------------------------------- |
| **G-Counter**    | `counter::GCounter`        | Grow-only counter med per-replika tellinger og elementvis max merge. |
| **PN-Counter**   | `counter::PNCounter`       | Positive-negative counter konstruert fra to G-Counters.              |
| **LWW-Register** | `register::LwwRegister<T>` | Last-writer-wins register med Lamport-tidsstempler.                  |
| **OR-Set**       | `set::OrSet<T>`            | Observed-Remove Set med add-wins semantikk.                          |
| **RGA**          | `text::Rga`                | Replicated Growable Array — tekst-CRDT med stabile posisjons-ID-er.  |


### Støttekomponenter

- `**clock::LamportClock`** og `clock::LamportTimestamp`. Det er logiske klokker med total ordning via `(counter, replica_id)` tie-breaking.
- `Crdt`**-trait**. Et felles grensesnitt for alle state-based CRDTs.

### Demoapplikasjon

- Peer-to-peer noder som synkroniserer en delt `OrSet<String>` (handleliste).
- Asynkron arkitektur basert på `tokio` med tre samtidige oppgaver per node: innkommende TCP-lytter, periodisk gossip-løkke, og interaktiv stdin-REPL.
- Lengde-prefiksert wire-protokoll med `bincode`-serialisering.
- Robust mot frakobling: noder kan startes, stoppes og restartes uten at det påvirker konvergensen.

### Verifikasjon

- **64 enhetstester** dekker normal bruk og andre edge cases.
- **19 property-tester** verifiserer kommutativitet, assosiativitet og idempotens for hver CRDT-type, samt sterk konvergens. Hver property kjøres mot 256 tilfeldig genererte tilfeller per `cargo test`-kjøring.
- **Streng CI**: tester på Linux, macOS og Windows; `clippy` med`pedantic + nursery`; `cargo fmt --check`; rustdoc med`-D warnings`; `unsafe_code = "forbid"` på hele workspacet.

---

## Fremtidig arbeid

Følgende er bevisst utelatt fra denne implementasjonen. Hvert punkt kan anses som en reell svakhet i forhold til et produksjonssystem:

- **Delta-state CRDTs.** Gossip-protokollen sender hele tilstanden i hver runde. Det er båndbreddesløsende, men illustrerer godt at merge er idempotent. En produksjonsversjon ville sendt kun delta siden forrige synkronisering ([Almeida m.fl., 2018](https://arxiv.org/abs/1603.01529)).
- **Tombstone garbage collection.** Både OR-Set og RGA akkumulerer tombstones uten å fjerne dem. Causal stability ([Bauwens & Boix](https://soft.vub.ac.be/Publications/2020/vub-soft-tr-20-04.pdf)) lar tombstones trygt fjernes når alle replikaer har sett dem.
- **Persistens.** All tilstand holdes i minnet og forsvinner når noden avsluttes. En reell node trenger skriving til disk og recovery ved  
oppstart.
- **Anti-entropi med Merkle-trær.** Periodisk full-state-gossip er enkelt, men skalerer dårlig. Et Merkle-tre-basert sammenligningsskjema ville la noder identifisere forskjeller i `O(log n)` istedenfor `O(n)`.
- **Autentisering og kryptering.** Demoapplikasjonen stoler blindt på innkommende koblinger. En reell distribusjon trenger TLS og en form for identitetsbasert autentisering.
- **RGA-ytelse.** Min implementasjon re-sorterer hele node-listen ved hver innsetting (blir `O(n log n))` per operasjon. Produksjonsbiblioteker som Yjs og Automerge bruker B-tre-baserte interne strukturer for `O(log n)`.
- **Flere CRDT-typer.** MV-Register (Multi-Value Register), 2P-Set, og causal trees er kjente varianter som dette prosjektet ikke implementerer, men som kunne vært kult.

---

## Eksterne avhengigheter

Alle CRDT-algoritmene er skrevet fra grunnen av. Avhengighetene som listes nedenfor leverer kun infrastruktur (serialisering,  
async I/O, testing, logging). 

### Kjørende avhengigheter


| Avhengighet                      | Brukt til                                                    |
| -------------------------------- | ------------------------------------------------------------ |
| `serde`                          | Avledet serialiserings-trait for alle CRDT-typer.            |
| `bincode`                        | Kompakt binærserialisering i wire-protokollen.               |
| `tokio`                          | Asynkron runtime for peer-to-peer-noden (TCP, timer, stdin). |
| `tracing` + `tracing-subscriber` | Strukturert logging i demoapplikasjonen.                     |
| `clap`                           | Kommandolinjeparser for demoapplikasjonens argumenter.       |
| `anyhow`                         | Ergonomisk feilhåndtering på applikasjonsnivå.               |


### Test-avhengigheter


| Avhengighet  | Brukt til                                                                                       |
| ------------ | ----------------------------------------------------------------------------------------------- |
| `proptest`   | Property-based testing for å verifisere CRDT-lovene over tusenvis av genererte input-tilfeller. |
| `serde_json` | Lesbar serialisering i utvalgte tester.                                                         |


---

## Installasjon

### Forutsetninger

- **Rust 1.75 eller nyere.** Installer via [rustup](https://rustup.rs).
- Et Unix-lignende terminalmiljø anbefales for demoen, men prosjektet bygger og kjører også på Windows.

### Bygg

```bash
git clone https://github.com/shizaahmad27/crusty-crdt.git
cd crusty-crdt
cargo build --release
```

Første bygg laster ned alle avhengigheter og tar 2–3 minutter. Påfølgende bygg skal være raske.

---

## Bruk

### Som bibliotek

Legg til i din egen `Cargo.toml`:

```toml
[dependencies]
crdt-lib = { path = "../crusty-crdt/crdt-lib" }
```

Et fullstendig eksempel:

```rust
use crdt_lib::counter::GCounter;
use crdt_lib::Crdt;

let mut a = GCounter::new();
let mut b = GCounter::new();

a.increment(1, 3);  // replika 1 inkrementerer 3 ganger
b.increment(2, 5);  // replika 2 inkrementerer 5 ganger

// Replikaer slår sammen tilstanden
a.merge(&b);
b.merge(&a);

assert_eq!(a.value(), 8);
assert_eq!(a, b);  // konvergens
```

For mer avansert bruk, se API-dokumentasjonen.

### Som demoapplikasjon

Start tre noder i hver sin terminal:

```bash
# Terminal 1
cargo run --release --bin crdt-demo -- \
  --id 1 --port 7001 --peers 127.0.0.1:7002,127.0.0.1:7003

# Terminal 2
cargo run --release --bin crdt-demo -- \
  --id 2 --port 7002 --peers 127.0.0.1:7001,127.0.0.1:7003

# Terminal 3
cargo run --release --bin crdt-demo -- \
  --id 3 --port 7003 --peers 127.0.0.1:7001,127.0.0.1:7002
```

Tilgjengelige kommandoer per node:


| Kommando                          | Effekt                                          |
| --------------------------------- | ----------------------------------------------- |
| `add <element>`                   | Legger til et element i den delte handlelisten. |
| `remove <element>` *(eller `rm`)* | Fjerner et element.                             |
| `list` *(eller `ls`)*             | Viser nåværende synlige elementer.              |
| `quit` *(eller `exit`)*           | Avslutter noden.                                |


Endringer propagerer automatisk hvert 2. sekund (justerbart med`--gossip-ms`). Noder kan trygt frakobles og kobles til igjen. Tilstanden konvergerer når kommunikasjonen er gjenopprettet.

---

## Kjøring av tester

```bash
# Alle tester i hele workspacet
cargo test --workspace

# Bare enhetstestene i biblioteket
cargo test -p crdt-lib --lib

# Bare property-testene (tar lengre tid)
cargo test -p crdt-lib --test properties

# Med ekstra antall property-cases (krever miljøvariabel)
PROPTEST_CASES=2048 cargo test -p crdt-lib --test properties
```

### Linting og formatering

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Doc-tester

Alle eksempelblokker i doc-kommentarer kjøres som tester:

```bash
cargo test --workspace --doc
```

---

## API-dokumentasjon

Publisert versjon: [https://shizaahmad27.github.io/crusty-crdt/](https://shizaahmad27.github.io/crusty-crdt/) (oppdateres automatisk av CI ved push til `main`)

Generer og åpne lokalt:

```bash
cargo doc --workspace --no-deps --open
```

---

## Prosjektstruktur

```
crusty-crdt/
├── Cargo.toml              workspace-konfigurasjon, lints
├── README.md
├── LICENSE                 MIT
├── rustfmt.toml
├── .github/workflows/ci.yml  test/clippy/fmt/docs på 3 OS
├── crdt-lib/               selve CRDT-biblioteket
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs          Crdt-trait, ReplicaId, modul-eksport
│   │   ├── clock.rs        Lamport-klokke
│   │   ├── counter.rs      G-Counter, PN-Counter
│   │   ├── register.rs     LWW-Register
│   │   ├── set.rs          OR-Set
│   │   └── text.rs         RGA tekst-CRDT
│   └── tests/
│       └── properties.rs   property-baserte verifikasjoner
└── crdt-demo/              peer-to-peer demoapplikasjon
    ├── Cargo.toml
    └── src/
        ├── main.rs         argument-parsing og REPL
        ├── node.rs         tilstand, gossip-løkke, peer-håndtering
        └── protocol.rs     wire-protokoll og serialisering
```

---

## Eksterne kilder

Følgende kilder har vært sentrale i utformingen av løsningen. Ingen kode er kopiert fra dem; algoritmene er reimplementert basert på publiserte beskrivelser.

### Fagartikler

- Shapiro, M., Preguiça, N., Baquero, C., & Zawirski, M. (2011). *A comprehensive study of Convergent and Commutative Replicated Data Types*. INRIA Research Report 7506. [https://hal.inria.fr/inria-00555588](https://hal.inria.fr/inria-00555588)
- Roh, H., Jeon, M., Kim, J., & Lee, J. (2011). *Replicated abstract data types: Building blocks for collaborative applications*. Journal of Parallel and Distributed Computing, 71(3).
- Lamport, L. (1978). *Time, Clocks, and the Ordering of Events in a Distributed System*. Communications of the ACM, 21(7).

### Foredrag og blogger

- Kleppmann, M. (2020). *CRDTs: The Hard Parts*. Hydra Conference.
[https://www.youtube.com/watch?v=x7drE24geUw](https://www.youtube.com/watch?v=x7drE24geUw)
- Sypytkowski, B. *An Introduction to State-based CRDTs*. Blog series.
[https://bartoszsypytkowski.com/the-state-of-a-state-based-crdts/](https://bartoszsypytkowski.com/the-state-of-a-state-based-crdts/)

### Verktøy og dokumentasjon

- [The Rust Programming Language](https://doc.rust-lang.org/book/) for generelle Rust-spørsmål.
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) for async-mønstre i demoapplikasjonen.
- [proptest documentation](https://docs.rs/proptest) for property-testing.

---

## Lisens

MIT — se [LICENSE](LICENSE).