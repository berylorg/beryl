#![cfg(feature = "lifecycle-test-support")]

#[path = "thread_injection_session/failures.rs"]
mod failures;
#[path = "thread_injection_session/fixtures.rs"]
mod fixtures;
#[path = "thread_injection_session/gates.rs"]
mod gates;
#[path = "thread_injection_session/lifecycle.rs"]
mod lifecycle;
#[path = "support/recovery_page.rs"]
mod recovery_page;
#[path = "request_flow/support.rs"]
mod support;
