#![cfg(feature = "test-faults")]

mod support;

#[path = "delivery_recovery/authority_lost_context.rs"]
mod authority_lost_context;
#[path = "delivery_recovery/classification.rs"]
mod classification;
#[path = "delivery_recovery/finalizing_history.rs"]
mod finalizing_history;
#[path = "delivery_recovery/finalizing_history_support.rs"]
mod finalizing_history_support;
#[path = "delivery_recovery/pages.rs"]
mod pages;
#[path = "delivery_recovery/projection_support.rs"]
mod projection_support;
#[path = "delivery_recovery/support.rs"]
mod recovery_support;
#[path = "delivery_recovery/source_pages.rs"]
mod source_pages;
