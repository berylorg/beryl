//! Production storage boundary for Syndic.
//!
//! This crate owns the reusable persistence API for Syndic-owned conversation
//! history, live source events, references, transcript views, and derived
//! projection state.
//!
//! The target storage engine is `fjall`. Public callers must use typed Syndic
//! storage APIs rather than depending on `fjall` keyspaces or encoded records.
//! The concrete API is intentionally not exposed until the
//! `cas-live-syndic-transcript` rework design is reviewed.
//!
//! This crate does not call model providers, Codex App Server, or OpenAI APIs,
//! and it must never persist authentication secrets.

#![forbid(unsafe_code)]
