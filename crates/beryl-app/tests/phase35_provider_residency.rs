#![cfg(feature = "test-faults")]

#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase35_provider_residency/failure.rs"]
mod failure;
#[path = "phase35_provider_residency/fixture.rs"]
mod fixture;
#[path = "phase35_provider_residency/generator.rs"]
mod generator;
#[path = "phase35_provider_residency/scale.rs"]
mod scale;
#[path = "phase35_provider_residency/server.rs"]
mod server;
#[path = "phase35_provider_residency/verification.rs"]
mod verification;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

static PHASE35_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn provider_scale_repetition_and_release_are_fixed() {
    let _guard = PHASE35_TEST_LOCK.lock().unwrap();
    scale::prove_scale_repetition_and_release();
}

#[test]
fn provider_transport_backpressure_and_cancellation_release() {
    let _guard = PHASE35_TEST_LOCK.lock().unwrap();
    fixture::prove_transport_backpressure_and_cancellation();
}

#[test]
fn provider_failures_and_unknown_outcomes_remain_atomic() {
    let _guard = PHASE35_TEST_LOCK.lock().unwrap();
    failure::prove_failure_release_and_atomic_visibility();
}
