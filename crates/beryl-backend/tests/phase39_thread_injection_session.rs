#![cfg(feature = "lifecycle-test-support")]

#[path = "phase39_thread_injection_session/failures.rs"]
mod failures;
#[path = "phase39_thread_injection_session/fixtures.rs"]
mod fixtures;
#[path = "phase39_thread_injection_session/gates.rs"]
mod gates;
#[path = "phase39_thread_injection_session/lifecycle.rs"]
mod lifecycle;
#[path = "support/recovery_page.rs"]
mod recovery_page;
#[path = "phase31_bounded_dispatch/support.rs"]
mod support;
