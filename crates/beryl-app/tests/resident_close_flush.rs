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
#[path = "resident_close_flush/recovery_clipboard.rs"]
mod recovery_clipboard;
#[path = "resident_close_flush/recovery_close_cleanup.rs"]
mod recovery_close_cleanup;
#[path = "resident_close_flush/recovery_close_completion.rs"]
mod recovery_close_completion;
#[path = "resident_close_flush/recovery_close_worker.rs"]
mod recovery_close_worker;
#[path = "resident_close_flush/recovery_configurator.rs"]
mod recovery_configurator;
#[path = "resident_close_flush/recovery_mount_service.rs"]
mod recovery_mount_service;
#[path = "resident_close_flush/recovery_mutation_failure.rs"]
mod recovery_mutation_failure;
#[path = "resident_close_flush/recovery_native_control.rs"]
mod recovery_native_control;
#[path = "resident_close_flush/recovery_native_lineage.rs"]
mod recovery_native_lineage;
#[path = "resident_close_flush/recovery_pending_cleanup.rs"]
mod recovery_pending_cleanup;
#[path = "resident_close_flush/recovery_resources.rs"]
mod recovery_resources;
#[path = "resident_close_flush/recovery_retirement.rs"]
mod recovery_retirement;
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

#[path = "resident_close_flush/recovery_native_disposal.rs"]
mod recovery_native_disposal;
