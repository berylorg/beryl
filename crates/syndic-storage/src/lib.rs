//! Production storage boundary for Syndic.
//!
//! This crate owns the reusable persistence API for Syndic-owned conversation
//! history, references, and derived projection state. The public API is still
//! intentionally empty while the feature and package contracts are being
//! established.
//!
//! Benchmark-only storage adapters and synthetic workload harnesses live in the
//! sibling `syndic-benchmarks` workspace rather than in Beryl.

#![forbid(unsafe_code)]
