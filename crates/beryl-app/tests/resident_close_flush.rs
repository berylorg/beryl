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
#[path = "resident_close_flush/support.rs"]
mod support;
