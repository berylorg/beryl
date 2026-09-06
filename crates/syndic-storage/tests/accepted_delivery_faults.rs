#![cfg(feature = "test-faults")]

mod support;

#[path = "accepted_delivery/fixtures.rs"]
mod accepted_fixtures;
#[path = "accepted_delivery/accepted_support.rs"]
mod accepted_support;
#[path = "accepted_delivery/faults.rs"]
mod faults;
