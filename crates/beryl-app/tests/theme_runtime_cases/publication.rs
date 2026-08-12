use std::{num::NonZeroU64, sync::Arc};

use beryl_app::theme_runtime::{
    AdapterRegistrationError, AppearanceGenerationNumber, DurablePublicationError,
    DurablePublicationOutcome, PreviewCandidateIdentity, PreviewSource, PreviewSourceIdentity,
    StalePublicationReason, WindowSetEpoch,
};
use beryl_state::{PreparedThemeAppearance, ThemeDocumentDigest};

use crate::support::{
    StateFixture, TestAdapter, coordinator, preview_completion, settings_identity,
};

#[test]
fn every_adapter_accepts_before_infallible_commit_and_all_receive_one_arc() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 2);
    let first = TestAdapter::new(1);
    let second = TestAdapter::new(2);
    coordinator.register_adapter(first.clone()).unwrap();
    coordinator.register_adapter(second.clone()).unwrap();
    let initial = coordinator.current();
    let first_commits = first.state.commit_count();
    let second_commits = second.state.commit_count();

    second.state.reject(true);
    let replacement = fixture.prepared(2);
    let request = coordinator
        .begin_durable_publication(settings_identity(&replacement, 1, 1))
        .unwrap();
    assert!(matches!(
        coordinator.publish_durable(request, replacement.clone()),
        Err(DurablePublicationError::Adapter { .. })
    ));
    assert!(Arc::ptr_eq(&coordinator.current(), &initial));
    assert_eq!(first.state.commit_count(), first_commits);
    assert_eq!(second.state.commit_count(), second_commits);
    assert!(Arc::ptr_eq(&first.state.current(), &initial));
    assert!(Arc::ptr_eq(&second.state.current(), &initial));

    second.state.reject(false);
    let request = coordinator
        .begin_durable_publication(settings_identity(&replacement, 1, 2))
        .unwrap();
    let DurablePublicationOutcome::Published(published) =
        coordinator.publish_durable(request, replacement).unwrap()
    else {
        panic!("settings publication must be visible");
    };
    assert!(Arc::ptr_eq(&coordinator.current(), &published));
    assert!(Arc::ptr_eq(&coordinator.durable_base(), &published));
    assert!(Arc::ptr_eq(&first.state.current(), &published));
    assert!(Arc::ptr_eq(&second.state.current(), &published));
}

#[test]
fn adapter_epoch_churn_invalidates_work_and_new_adapter_is_ready_before_eligibility() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 3);
    let first = TestAdapter::new(1);
    coordinator.register_adapter(first.clone()).unwrap();
    let replacement = fixture.prepared(2);
    let request = coordinator
        .begin_durable_publication(settings_identity(&replacement, 1, 1))
        .unwrap();
    let prior = coordinator.current();

    let newcomer = TestAdapter::new(2);
    coordinator.register_adapter(newcomer.clone()).unwrap();
    assert!(Arc::ptr_eq(&newcomer.state.current(), &prior));
    assert_eq!(coordinator.diagnostics().adapter_count(), 2);
    assert!(matches!(
        coordinator.publish_durable(request, replacement),
        Err(DurablePublicationError::Stale(
            StalePublicationReason::WindowSetEpoch
        ))
    ));
    assert!(Arc::ptr_eq(&first.state.current(), &prior));
    assert!(Arc::ptr_eq(&newcomer.state.current(), &prior));
}

#[test]
fn stale_generation_draft_and_candidate_identity_are_rejected() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    coordinator.register_adapter(adapter).unwrap();

    let durable = fixture.prepared(2);
    let stale_generation = coordinator
        .begin_durable_publication(settings_identity(&durable, 1, 1))
        .unwrap();
    let preview_request = coordinator
        .begin_preview(
            PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(1).unwrap()),
            PreviewCandidateIdentity::Digest(ThemeDocumentDigest::of_bytes(b"preview")),
        )
        .unwrap();
    let completion = preview_completion(&preview_request, fixture.prepared(1));
    coordinator
        .publish_preview(preview_request, completion)
        .unwrap();
    assert!(matches!(
        coordinator.publish_durable(stale_generation, durable),
        Err(DurablePublicationError::Stale(
            StalePublicationReason::CurrentGeneration
        ))
    ));

    let first = fixture.prepared(3);
    let stale_draft = coordinator
        .begin_durable_publication(settings_identity(&first, 1, 1))
        .unwrap();
    let second = fixture.prepared(4);
    let _newer_draft = coordinator
        .begin_durable_publication(settings_identity(&second, 2, 1))
        .unwrap();
    assert!(matches!(
        coordinator.publish_durable(stale_draft, first),
        Err(DurablePublicationError::Stale(
            StalePublicationReason::DurableAttempt
        ))
    ));

    let expected = fixture.prepared(5);
    let request = coordinator
        .begin_durable_publication(settings_identity(&expected, 3, 1))
        .unwrap();
    assert!(matches!(
        coordinator.publish_durable(request, fixture.prepared(6)),
        Err(DurablePublicationError::CandidateMismatch)
    ));
}

#[test]
fn fresh_service_identity_fences_old_requests_and_foreign_candidates() {
    let fixture = StateFixture::new();
    let mut old = coordinator(&fixture, 1);
    let prepared = fixture.prepared(2);
    let request = old
        .begin_durable_publication(settings_identity(&prepared, 1, 1))
        .unwrap();

    let fresh_service = fixture.fresh_service();
    let fresh_settings =
        fresh_service.settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None);
    let fresh_prepared = PreparedThemeAppearance::fallback(fresh_settings);
    let mut fresh = beryl_app::theme_runtime::AppearanceCoordinator::new(
        beryl_app::theme_runtime::AppearanceCoordinatorConfig::new(
            std::num::NonZeroUsize::new(1).unwrap(),
        ),
        fresh_prepared.clone(),
    );
    assert!(matches!(
        fresh.publish_durable(request, prepared),
        Err(DurablePublicationError::Stale(
            StalePublicationReason::ForeignService
        ))
    ));
    assert!(matches!(
        old.begin_durable_publication(settings_identity(&fresh_prepared, 2, 1)),
        Err(DurablePublicationError::Stale(
            StalePublicationReason::ForeignService
        ))
    ));
}

#[test]
fn monotonic_identity_domains_report_overflow_without_wrapping() {
    let first = AppearanceGenerationNumber::initial();
    assert!(first.checked_next().unwrap() > first);
    assert!(
        AppearanceGenerationNumber::from_nonzero(NonZeroU64::MAX)
            .checked_next()
            .is_err()
    );
    assert!(
        WindowSetEpoch::from_nonzero(NonZeroU64::MAX)
            .checked_next()
            .is_err()
    );
}

#[test]
fn capacity_and_adapter_ownership_are_bounded_and_released() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    let weak = Arc::downgrade(&adapter);
    coordinator.register_adapter(adapter.clone()).unwrap();
    assert_eq!(
        coordinator.register_adapter(TestAdapter::new(2)),
        Err(AdapterRegistrationError::CapacityReached)
    );
    let id = adapter.id();
    drop(adapter);
    assert!(coordinator.unregister_adapter(id).unwrap());
    assert!(weak.upgrade().is_none());
    assert_eq!(coordinator.diagnostics().adapter_count(), 0);
}
