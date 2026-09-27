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
#[path = "resident_close_flush/recovery_autosave.rs"]
mod recovery_autosave;
#[path = "resident_close_flush/recovery_close_cleanup.rs"]
mod recovery_close_cleanup;
#[path = "resident_close_flush/recovery_close_completion.rs"]
mod recovery_close_completion;
#[path = "resident_close_flush/recovery_close_worker.rs"]
mod recovery_close_worker;
#[path = "resident_close_flush/recovery_configurator.rs"]
mod recovery_configurator;
#[path = "resident_close_flush/recovery_native_lineage.rs"]
mod recovery_native_lineage;
#[path = "resident_close_flush/recovery_submission.rs"]
mod recovery_submission;
#[path = "resident_close_flush/recovery_submission_worker.rs"]
mod recovery_submission_worker;
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
