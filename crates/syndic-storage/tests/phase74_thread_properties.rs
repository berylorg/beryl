#![cfg(feature = "test-faults")]

mod support;

#[path = "phase74_thread_properties/archive.rs"]
mod archive;
#[path = "phase74_thread_properties/catalog_fences.rs"]
mod catalog_fences;
#[path = "phase74_thread_properties/core.rs"]
mod core;
#[path = "phase74_thread_properties/title_usage.rs"]
mod title_usage;
