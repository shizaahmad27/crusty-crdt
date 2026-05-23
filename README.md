# crusty-crdt

[![CI](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml/badge.svg)](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-rustdoc-blue)](https://shizaahmad27.github.io/crusty-crdt/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> A Rust library of Conflict-free Replicated Data Types (CRDTs), implemented
> from scratch as a bachelor's project in distributed systems. Includes five
> CRDT types and a peer-to-peer demo application.

**Latest CI run:** [GitHub Actions](https://github.com/shizaahmad27/crusty-crdt/actions/workflows/ci.yml) (badge above shows status for the latest run on `main`)

**API documentation:** [shizaahmad27.github.io/crusty-crdt](https://shizaahmad27.github.io/crusty-crdt/)

---

## Introduction

Distributed applications like Google Docs, Figma, and Apple Notes let multiple users edit the same data simultaneously, even while offline. When changes are reconciled, a fundamental problem arises: how do you resolve conflicts between concurrent edits, deterministically, without a central authority?

*Conflict-free Replicated Data Types* (CRDTs) solve this by construction. Each node can update its local copy without coordination, and a mathematically guaranteed convergence ensures all nodes end up in the same state once they exchange updates. The guarantee rests on `merge` being commutative, associative, and idempotent — properties that together make the state space a *join-semilattice*.

This project implements five state-based CRDTs from scratch in Rust, verifies each with property-based testing, and demonstrates them in a peer-to-peer application with a gossip protocol over TCP. All CRDT properties are machine-verified through ~4900 automatically generated test cases per run.

---

## Implemented functionality

### CRDT types

| Type             | Module                     | Description                                                        |
| ---------------- | -------------------------- | ------------------------------------------------------------------ |
| **G-Counter**    | `counter::GCounter`        | Grow-only counter with per-replica counts and element-wise max merge. |
| **PN-Counter**   | `counter::PNCounter`       | Positive-negative counter built from two G-Counters.               |
| **LWW-Register** | `register::LwwRegister<T>` | Last-writer-wins register using Lamport timestamps.                |
| **OR-Set**       | `set::OrSet<T>`            | Observed-Remove Set with add-wins semantics.                       |
| **RGA**          | `text::Rga`                | Replicated Growable Array — a text CRDT with stable position IDs.  |

### Supporting components

- **`clock::LamportClock`** and `clock::LamportTimestamp` — logical clocks with total ordering via `(counter, replica_id)` tie-breaking.
- **`Crdt` trait** — a common interface for all state-based CRDTs.

### Demo application

- Peer-to-peer nodes synchronizing a shared `OrSet<String>` (shopping list).
- Async architecture using `tokio` with three concurrent tasks per node: incoming TCP listener, periodic gossip loop, and an interactive stdin REPL.
- Length-prefixed wire protocol with `bincode` serialization.
- Resilient to disconnection: nodes can be started, stopped, and restarted without affecting convergence.

### Verification

- **64 unit tests** covering normal use and edge cases.
- **19 property tests** verifying commutativity, associativity, and idempotence for each CRDT type, plus strong convergence. Each property runs against 256 randomly generated cases per `cargo test` invocation.
- **Strict CI**: tested on Linux, macOS, and Windows; `clippy` with `pedantic + nursery`; `cargo fmt --check`; rustdoc with `-D warnings`; `unsafe_code = "forbid"` across the whole workspace.

---

## Future work

The following are intentionally omitted. Each can be considered a real weakness relative to a production system:

- **Delta-state CRDTs.** The gossip protocol sends the full state every round. This illustrates that merge is idempotent, but wastes bandwidth. A production version would send only the delta since the last sync ([Almeida et al., 2018](https://arxiv.org/abs/1603.01529)).
- **Tombstone garbage collection.** Both OR-Set and RGA accumulate tombstones indefinitely. Causal stability ([Bauwens & Boix](https://soft.vub.ac.be/Publications/2020/vub-soft-tr-20-04.pdf)) allows tombstones to be safely removed once all replicas have seen them.
- **Persistence.** All state is in-memory and lost when the node exits. A real node needs disk writes and recovery on startup.
- **Anti-entropy with Merkle trees.** Full-state gossip is simple but doesn't scale. A Merkle-tree-based comparison scheme would let nodes identify differences in `O(log n)` rather than `O(n)`.
- **Authentication and encryption.** The demo trusts all incoming connections blindly. A real deployment needs TLS and identity-based authentication.
- **RGA performance.** The implementation re-sorts the full node list on each insertion (`O(n log n)` per operation). Production libraries like Yjs and Automerge use B-tree-based internal structures for `O(log n)`.
- **More CRDT types.** MV-Register, 2P-Set, and causal trees are well-known variants not covered here.

---

## External dependencies

All CRDT algorithms are written from scratch. The dependencies below provide infrastructure only (serialization, async I/O, testing, logging).

### Runtime dependencies

| Dependency                       | Used for                                                       |
| -------------------------------- | -------------------------------------------------------------- |
| `serde`                          | Derived serialization traits for all CRDT types.               |
| `bincode`                        | Compact binary serialization in the wire protocol.             |
| `tokio`                          | Async runtime for the peer-to-peer node (TCP, timers, stdin).  |
| `tracing` + `tracing-subscriber` | Structured logging in the demo application.                    |
| `clap`                           | CLI argument parsing for the demo application.                 |
| `anyhow`                         | Ergonomic error handling at the application level.             |

### Test dependencies

| Dependency   | Used for                                                                                   |
| ------------ | ------------------------------------------------------------------------------------------ |
| `proptest`   | Property-based testing to verify CRDT laws over thousands of generated input cases.        |
| `serde_json` | Human-readable serialization in selected tests.                                            |

---

## Installation

### Prerequisites

- **Rust 1.75 or later.** Install via [rustup](https://rustup.rs).
- A Unix-like terminal is recommended for the demo, but the project builds and runs on Windows too.

### Build

```bash
git clone https://github.com/shizaahmad27/crusty-crdt.git
cd crusty-crdt
cargo build --release
```

---

## Usage

### As a library

Add to your `Cargo.toml`:

```toml
[dependencies]
crdt-lib = { path = "../crusty-crdt/crdt-lib" }
```

A complete example:

```rust
use crdt_lib::counter::GCounter;
use crdt_lib::Crdt;

let mut a = GCounter::new();
let mut b = GCounter::new();

a.increment(1, 3);  // replica 1 increments by 3
b.increment(2, 5);  // replica 2 increments by 5

// Replicas merge state
a.merge(&b);
b.merge(&a);

assert_eq!(a.value(), 8);
assert_eq!(a, b);  // convergence
```

For more advanced usage, see the API documentation.

### As a demo application

Start three nodes in separate terminals:

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

Available commands per node:

| Command                          | Effect                                    |
| -------------------------------- | ----------------------------------------- |
| `add <item>`                     | Adds an item to the shared shopping list. |
| `remove <item>` *(or `rm`)*      | Removes an item.                          |
| `list` *(or `ls`)*               | Shows the current visible items.          |
| `quit` *(or `exit`)*             | Exits the node.                           |

Changes propagate automatically every 2 seconds (adjustable with `--gossip-ms`). Nodes can be safely disconnected and reconnected; state converges once communication is restored.

---

## Running tests

```bash
# All tests in the entire workspace
cargo test --workspace

# Unit tests only
cargo test -p crdt-lib --lib

# Property tests only
cargo test -p crdt-lib --test properties

# With a higher property case count
PROPTEST_CASES=2048 cargo test -p crdt-lib --test properties
```

### Linting and formatting

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Doc tests

All code blocks in doc comments are run as tests:

```bash
cargo test --workspace --doc
```

---

## API documentation

Published: [https://shizaahmad27.github.io/crusty-crdt/](https://shizaahmad27.github.io/crusty-crdt/) (updated automatically by CI on push to `main`)

Generate and open locally:

```bash
cargo doc --workspace --no-deps --open
```

---

## Project structure

```
crusty-crdt/
├── Cargo.toml              workspace configuration, lints
├── README.md
├── LICENSE                 MIT
├── rustfmt.toml
├── .github/workflows/ci.yml  test/clippy/fmt/docs on 3 OSes
├── crdt-lib/               the CRDT library
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs          Crdt trait, ReplicaId, module exports
│   │   ├── clock.rs        Lamport clock
│   │   ├── counter.rs      G-Counter, PN-Counter
│   │   ├── register.rs     LWW-Register
│   │   ├── set.rs          OR-Set
│   │   └── text.rs         RGA text CRDT
│   └── tests/
│       └── properties.rs   property-based verifications
└── crdt-demo/              peer-to-peer demo application
    ├── Cargo.toml
    └── src/
        ├── main.rs         argument parsing and REPL
        ├── node.rs         state, gossip loop, peer handling
        └── protocol.rs     wire protocol and serialization
```

---

## References

No code was copied from any of the following sources; algorithms were reimplemented from published descriptions.

### Papers

- Shapiro, M., Preguiça, N., Baquero, C., & Zawirski, M. (2011). *A comprehensive study of Convergent and Commutative Replicated Data Types*. INRIA Research Report 7506. [https://hal.inria.fr/inria-00555588](https://hal.inria.fr/inria-00555588)
- Roh, H., Jeon, M., Kim, J., & Lee, J. (2011). *Replicated abstract data types: Building blocks for collaborative applications*. Journal of Parallel and Distributed Computing, 71(3).
- Lamport, L. (1978). *Time, Clocks, and the Ordering of Events in a Distributed System*. Communications of the ACM, 21(7).

### Talks and blogs

- Kleppmann, M. (2020). *CRDTs: The Hard Parts*. Hydra Conference.
[https://www.youtube.com/watch?v=x7drE24geUw](https://www.youtube.com/watch?v=x7drE24geUw)
- Sypytkowski, B. *An Introduction to State-based CRDTs*. Blog series.
[https://bartoszsypytkowski.com/the-state-of-a-state-based-crdts/](https://bartoszsypytkowski.com/the-state-of-a-state-based-crdts/)

### Tools and documentation

- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [proptest documentation](https://docs.rs/proptest)

---

## License

MIT — see [LICENSE](LICENSE).
