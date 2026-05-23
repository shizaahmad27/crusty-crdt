//! # crdt-lib
//!
//! State-based Conflict-free Replicated Data Types (CRDTs) implemented from
//! scratch in Rust. Each CRDT's `merge` is commutative, associative, and
//! idempotent, guaranteeing convergence regardless of message order or duplication.
//!
//! ## Types
//!
//! - [`counter::GCounter`] — grow-only counter
//! - [`counter::PNCounter`] — counter supporting increment and decrement
//! - [`register::LwwRegister`] — last-writer-wins register
//! - [`set::OrSet`] — observed-remove set with add-wins semantics
//! - [`text::Rga`] — replicated growable array for collaborative text

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod clock;
pub mod counter;
pub mod register;
pub mod set;
pub mod text;

/// Trait implemented by all state-based CRDTs in this library.
///
/// Implementations must guarantee that `merge` is commutative, associative, and
/// idempotent. These properties are verified by property-based tests in
/// `tests/properties.rs`.
pub trait Crdt {
    /// Merges `other` into `self`, producing the join of the two states in the lattice.
    fn merge(&mut self, other: &Self);
}

/// Unique identifier for a node in a CRDT system.
///
/// Must be globally unique across all replicas. This library uses `u64`;
/// a production system would typically use a UUID.
pub type ReplicaId = u64;
