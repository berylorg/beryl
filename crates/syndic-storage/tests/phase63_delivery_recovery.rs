#![cfg(feature = "test-faults")]

mod support;

#[path = "phase63_delivery_recovery/authority_lost_context.rs"]
mod authority_lost_context;
#[path = "phase63_delivery_recovery/classification.rs"]
mod classification;
#[path = "phase63_delivery_recovery/finalizing_history.rs"]
mod finalizing_history;
#[path = "phase63_delivery_recovery/finalizing_history_support.rs"]
mod finalizing_history_support;
#[path = "phase63_delivery_recovery/pages.rs"]
mod pages;
#[path = "phase63_delivery_recovery/projection_support.rs"]
mod projection_support;
#[path = "phase63_delivery_recovery/support.rs"]
mod recovery_support;
