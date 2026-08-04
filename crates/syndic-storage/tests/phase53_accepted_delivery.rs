#![cfg(feature = "test-faults")]

mod support;

#[path = "phase53_accepted_delivery/accepted_support.rs"]
mod accepted_support;
#[path = "phase53_accepted_delivery/reads.rs"]
mod reads;
#[path = "phase53_accepted_delivery/reconciliation.rs"]
mod reconciliation;
#[path = "phase53_accepted_delivery/transitions.rs"]
mod transitions;
