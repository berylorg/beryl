#[path = "historical_root_adoption/default.rs"]
mod default;
#[cfg(feature = "test-faults")]
#[path = "historical_root_adoption/faults.rs"]
mod faults;
#[path = "historical_root_adoption/support.rs"]
mod support;
