#![cfg(feature = "test-faults")]
#![allow(unused_imports, dead_code)]

include!("durable_builder/support.rs");

#[path = "marker_continuation/support.rs"]
mod continuation_support;
#[path = "marker_continuation/endpoint.rs"]
mod endpoint;
#[path = "marker_continuation/integrity.rs"]
mod integrity;
#[path = "marker_continuation/restart.rs"]
mod restart;
#[path = "marker_continuation/semantics.rs"]
mod semantics;
