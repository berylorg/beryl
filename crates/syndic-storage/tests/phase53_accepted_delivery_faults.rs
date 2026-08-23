#![cfg(feature = "test-faults")]

mod support;

#[path = "phase53_accepted_delivery/fixtures.rs"]
mod accepted_fixtures;
#[path = "phase53_accepted_delivery/accepted_support.rs"]
mod accepted_support;
#[path = "phase53_accepted_delivery/faults.rs"]
mod faults;
