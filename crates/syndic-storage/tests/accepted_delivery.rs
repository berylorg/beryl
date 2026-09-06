#![cfg(feature = "test-faults")]

mod support;

#[path = "accepted_delivery/fixtures.rs"]
mod accepted_fixtures;
#[path = "accepted_delivery/accepted_support.rs"]
mod accepted_support;
#[path = "accepted_delivery/reads.rs"]
mod reads;
#[path = "accepted_delivery/reconciliation.rs"]
mod reconciliation;
#[path = "accepted_delivery/transitions.rs"]
mod transitions;
