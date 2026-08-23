#[cfg(feature = "test-faults")]
mod support;

#[path = "phase65_stop_storage/support.rs"]
mod stop_support;

#[path = "phase65_stop_storage/lifecycle.rs"]
mod lifecycle;

#[path = "phase65_stop_storage/recovery.rs"]
mod recovery;

#[path = "phase65_stop_storage/abandonment.rs"]
mod abandonment;

#[cfg(feature = "test-faults")]
#[path = "phase65_stop_storage/queued.rs"]
mod queued;

#[cfg(feature = "test-faults")]
#[path = "phase65_stop_storage/corruption.rs"]
mod corruption;

#[cfg(feature = "test-faults")]
#[path = "phase65_stop_storage/faults.rs"]
mod faults;
