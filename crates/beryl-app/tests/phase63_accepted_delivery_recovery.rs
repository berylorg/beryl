#![cfg(feature = "test-faults")]
#![allow(dead_code)]

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[path = "phase62_accepted_next_scheduler/support.rs"]
mod phase62_support;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase63_accepted_delivery_recovery/active.rs"]
mod active;
#[path = "phase63_accepted_delivery_recovery/support.rs"]
mod app_support;
#[path = "phase63_accepted_delivery_recovery/availability.rs"]
mod availability;
#[path = "phase63_accepted_delivery_recovery/finalizing_history.rs"]
mod finalizing_history;
#[path = "phase63_accepted_delivery_recovery/paging.rs"]
mod paging;
#[path = "phase63_accepted_delivery_recovery/pending.rs"]
mod pending;
#[path = "phase63_accepted_delivery_recovery/progress.rs"]
mod progress;
#[path = "phase63_accepted_delivery_recovery/projection_refusal.rs"]
mod projection_refusal;
#[path = "phase63_accepted_delivery_recovery/records.rs"]
mod records;
#[path = "phase63_accepted_delivery_recovery/restart_cuts.rs"]
mod restart_cuts;
#[path = "phase63_accepted_delivery_recovery/revision_drift.rs"]
mod revision_drift;
