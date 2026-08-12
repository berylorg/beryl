use std::sync::Arc;

use beryl_app::theme_runtime::AdapterRegistrationError;

use crate::support::{StateFixture, TestAdapter, coordinator};

#[test]
fn rejected_registration_never_changes_epoch_or_eligibility() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let epoch = coordinator.diagnostics().window_epoch();
    let current = coordinator.current();
    let adapter = TestAdapter::new(1);
    adapter.state.reject(true);

    assert!(matches!(
        coordinator.register_adapter(adapter.clone()),
        Err(AdapterRegistrationError::Preparation { .. })
    ));
    assert_eq!(coordinator.diagnostics().window_epoch(), epoch);
    assert_eq!(coordinator.diagnostics().adapter_count(), 0);
    assert_eq!(adapter.state.commit_count(), 0);
    assert!(Arc::ptr_eq(&coordinator.current(), &current));
}

#[test]
fn unknown_unregistration_is_a_noop_for_window_epoch() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let epoch = coordinator.diagnostics().window_epoch();
    let unknown = TestAdapter::new(9);
    assert!(!coordinator.unregister_adapter(unknown.id()).unwrap());
    assert_eq!(coordinator.diagnostics().window_epoch(), epoch);
}
