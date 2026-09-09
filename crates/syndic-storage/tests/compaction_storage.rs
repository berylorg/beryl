#[cfg(feature = "test-faults")]
mod support;

#[path = "compaction_storage/admission.rs"]
mod admission;
#[path = "compaction_storage/support.rs"]
mod compaction_support;
#[cfg(feature = "test-faults")]
#[path = "compaction_storage/corruption.rs"]
mod corruption;
#[path = "compaction_storage/lifecycle.rs"]
mod lifecycle;
#[path = "compaction_storage/provider_stop.rs"]
mod provider_stop;
#[path = "compaction_storage/recovery.rs"]
mod recovery;
#[path = "compaction_storage/request_reconciliation.rs"]
mod request_reconciliation;
