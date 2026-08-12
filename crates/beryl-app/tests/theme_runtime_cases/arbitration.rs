use std::{num::NonZeroU64, sync::Arc};

use beryl_app::theme_runtime::{
    AppearancePublication, DurablePublicationIdentity, DurablePublicationOutcome,
    PreparedPreviewAppearance, PreviewCandidateIdentity, PreviewPublicationError, PreviewSequence,
    PreviewSource, PreviewSourceIdentity, StalePublicationReason, StopPreviewResult,
};
use beryl_state::{ThemeDocumentDigest, ThemeDraftIdentity, ThemeDraftRevision};

use crate::support::{
    StateFixture, TestAdapter, coordinator, preview_completion, settings_identity,
};

fn source(value: u64) -> PreviewSource {
    PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(value).unwrap())
}

fn candidate(bytes: &[u8]) -> PreviewCandidateIdentity {
    PreviewCandidateIdentity::Digest(ThemeDocumentDigest::of_bytes(bytes))
}

#[test]
fn later_preview_supersedes_earlier_and_latest_failure_never_revives_it() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    coordinator.register_adapter(adapter.clone()).unwrap();
    let durable = coordinator.current();

    let first = coordinator
        .begin_preview(source(1), candidate(b"first"))
        .unwrap();
    let second = coordinator
        .begin_preview(source(2), candidate(b"second"))
        .unwrap();
    let first_completion = preview_completion(&first, fixture.prepared(1));
    assert!(matches!(
        coordinator.publish_preview(first, first_completion),
        Err(PreviewPublicationError::Stale(
            StalePublicationReason::PreviewSequence
        ))
    ));

    adapter.state.reject(true);
    let second_completion = preview_completion(&second, fixture.prepared(1));
    assert!(matches!(
        coordinator.publish_preview(second, second_completion),
        Err(PreviewPublicationError::Adapter { .. })
    ));
    assert!(Arc::ptr_eq(&coordinator.current(), &durable));
    assert!(coordinator.diagnostics().pending_preview().is_none());
}

#[test]
fn successful_preview_replacement_is_direct_and_stop_restores_monotonically() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    coordinator.register_adapter(adapter.clone()).unwrap();
    let durable = coordinator.current();

    let first_request = coordinator
        .begin_preview(source(1), candidate(b"first"))
        .unwrap();
    let first_completion = preview_completion(&first_request, fixture.prepared(1));
    let first = coordinator
        .publish_preview(first_request, first_completion)
        .unwrap()
        .generation()
        .clone();
    let second_request = coordinator
        .begin_preview(source(2), candidate(b"second"))
        .unwrap();
    let second_completion = preview_completion(&second_request, fixture.prepared(1));
    let second = coordinator
        .publish_preview(second_request, second_completion)
        .unwrap()
        .generation()
        .clone();
    assert!(second.number() > first.number());
    let history = adapter.state.history();
    assert!(Arc::ptr_eq(&history[history.len() - 2], &first));
    assert!(Arc::ptr_eq(&history[history.len() - 1], &second));
    assert!(!Arc::ptr_eq(&history[history.len() - 1], &durable));

    adapter.state.reject(true);
    assert!(matches!(
        coordinator.stop_preview(),
        Err(PreviewPublicationError::Adapter { .. })
    ));
    assert!(Arc::ptr_eq(&coordinator.current(), &second));
    adapter.state.reject(false);
    let StopPreviewResult::Restored(restored) = coordinator.stop_preview().unwrap() else {
        panic!("preview must restore");
    };
    assert!(restored.number() > second.number());
    assert!(Arc::ptr_eq(&coordinator.current(), &restored));
    assert!(Arc::ptr_eq(&coordinator.durable_base(), &restored));
    assert!(matches!(
        restored.publication(),
        AppearancePublication::Durable
    ));
}

#[test]
fn hidden_durable_replacements_keep_preview_and_stop_uses_newest_base_arc() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    coordinator.register_adapter(adapter).unwrap();
    let preview_request = coordinator
        .begin_preview(source(1), candidate(b"preview"))
        .unwrap();
    let preview_completion = preview_completion(&preview_request, fixture.prepared(1));
    let preview = coordinator
        .publish_preview(preview_request, preview_completion)
        .unwrap()
        .generation()
        .clone();

    let base_two = fixture.prepared(2);
    let request = coordinator
        .begin_durable_publication(DurablePublicationIdentity::RepositoryRefresh(
            base_two.source().clone(),
        ))
        .unwrap();
    assert!(matches!(
        coordinator.publish_durable(request, base_two),
        Ok(DurablePublicationOutcome::HiddenBaseReplaced(_))
    ));
    let base_three = fixture.prepared(3);
    let request = coordinator
        .begin_durable_publication(DurablePublicationIdentity::RepositoryRefresh(
            base_three.source().clone(),
        ))
        .unwrap();
    let DurablePublicationOutcome::HiddenBaseReplaced(newest) =
        coordinator.publish_durable(request, base_three).unwrap()
    else {
        panic!("preview must hide a durable replacement");
    };
    assert!(Arc::ptr_eq(&coordinator.current(), &preview));
    assert_eq!(
        coordinator.durable_base().prepared().settings(),
        newest.prepared().settings()
    );

    let StopPreviewResult::Restored(restored) = coordinator.stop_preview().unwrap() else {
        panic!("preview must restore");
    };
    assert!(Arc::ptr_eq(&restored, &newest));
    assert!(Arc::ptr_eq(&coordinator.current(), &newest));
    assert!(Arc::ptr_eq(&coordinator.durable_base(), &newest));
}

#[test]
fn successful_settings_publication_ends_current_and_pending_preview() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let adapter = TestAdapter::new(1);
    coordinator.register_adapter(adapter).unwrap();
    let first = coordinator
        .begin_preview(source(1), candidate(b"first"))
        .unwrap();
    let first_completion = preview_completion(&first, fixture.prepared(1));
    coordinator
        .publish_preview(first, first_completion)
        .unwrap();
    let pending = coordinator
        .begin_preview(source(2), candidate(b"pending"))
        .unwrap();
    let replacement = fixture.prepared(2);
    let settings = coordinator
        .begin_durable_publication(settings_identity(&replacement, 1, 1))
        .unwrap();
    let DurablePublicationOutcome::Published(published) =
        coordinator.publish_durable(settings, replacement).unwrap()
    else {
        panic!("settings must publish");
    };
    assert!(Arc::ptr_eq(&coordinator.current(), &published));
    assert!(Arc::ptr_eq(&coordinator.durable_base(), &published));
    assert!(coordinator.diagnostics().current_preview().is_none());
    assert!(coordinator.diagnostics().pending_preview().is_none());
    let pending_completion = preview_completion(&pending, fixture.prepared(1));
    assert!(matches!(
        coordinator.publish_preview(pending, pending_completion),
        Err(PreviewPublicationError::Stale(_))
    ));
}

#[test]
fn preview_sequence_overflow_is_typed_and_source_identity_rejects_zero() {
    assert!(
        PreviewSequence::from_nonzero(NonZeroU64::MAX)
            .checked_next()
            .is_err()
    );
    assert!(PreviewSourceIdentity::try_new(0).is_err());
}

#[test]
fn preview_completion_rejects_a_different_draft_revision_or_digest() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let draft = ThemeDraftIdentity::new(NonZeroU64::new(1).unwrap());
    let expected = PreviewCandidateIdentity::Draft {
        draft,
        revision: ThemeDraftRevision::new(NonZeroU64::new(1).unwrap()),
    };
    let request = coordinator.begin_preview(source(1), expected).unwrap();
    let different_revision = PreviewCandidateIdentity::Draft {
        draft,
        revision: ThemeDraftRevision::new(NonZeroU64::new(2).unwrap()),
    };
    assert!(matches!(
        coordinator.publish_preview(
            request,
            PreparedPreviewAppearance::new(different_revision, fixture.prepared(1)),
        ),
        Err(PreviewPublicationError::CandidateMismatch)
    ));

    let request = coordinator
        .begin_preview(source(2), candidate(b"expected"))
        .unwrap();
    assert!(matches!(
        coordinator.publish_preview(
            request,
            PreparedPreviewAppearance::new(candidate(b"different"), fixture.prepared(1)),
        ),
        Err(PreviewPublicationError::CandidateMismatch)
    ));
}

#[test]
fn diagnostics_are_bounded_content_free_and_retirement_releases_generations() {
    let fixture = StateFixture::new();
    let mut coordinator = coordinator(&fixture, 1);
    let request = coordinator
        .begin_preview(source(987_654), candidate(b"secret candidate bytes"))
        .unwrap();
    let diagnostic = format!("{:?}", coordinator.diagnostics());
    assert!(!diagnostic.contains("987654"));
    assert!(!diagnostic.contains("secret"));
    drop(request);

    let current = coordinator.current();
    let weak = Arc::downgrade(&current);
    drop(current);
    coordinator.retire();
    assert!(weak.upgrade().is_none());
}
