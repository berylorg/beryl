#![cfg(feature = "test-faults")]

#[path = "syndic_composer_history/support.rs"]
mod composer;
#[path = "pending_composer_activation/support.rs"]
mod widget_support;

#[path = "resident_close_flush/final_disposal.rs"]
mod final_disposal;
#[path = "resident_close_flush/host.rs"]
mod host;
#[path = "resident_close_flush/mounted.rs"]
mod mounted;
#[path = "resident_close_flush/mutation.rs"]
mod mutation;
#[path = "resident_close_flush/recovery.rs"]
mod recovery;
#[path = "resident_close_flush/release.rs"]
mod release;
#[path = "resident_close_flush/retirement.rs"]
mod retirement;
#[path = "resident_close_flush/service_retirement.rs"]
mod service_retirement;
#[path = "resident_close_flush/shutdown.rs"]
mod shutdown;
#[path = "resident_close_flush/slot_retirement.rs"]
mod slot_retirement;
#[path = "resident_close_flush/support.rs"]
mod support;
