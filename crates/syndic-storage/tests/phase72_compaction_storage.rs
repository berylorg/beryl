#[cfg(feature = "test-faults")]
mod support;

#[path = "phase72_compaction_storage/support.rs"]
mod compaction_support;
#[cfg(feature = "test-faults")]
#[path = "phase72_compaction_storage/corruption.rs"]
mod corruption;
#[path = "phase72_compaction_storage/lifecycle.rs"]
mod lifecycle;
#[path = "phase72_compaction_storage/provider_stop.rs"]
mod provider_stop;
#[path = "phase72_compaction_storage/recovery.rs"]
mod recovery;
#[path = "phase72_compaction_storage/request_reconciliation.rs"]
mod request_reconciliation;
