use super::*;
use crate::cas_projection::{
    RetainedSelectedTranscriptOutcome, SelectedTranscriptConstructionError,
    SelectedTranscriptConstructionProgress,
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::*;

#[path = "../../pending_composer_activation/support.rs"]
mod composer_support;
#[path = "selected_construction/history.rs"]
mod history;

const CONTEXT: &str = "A genuine assistant discussion parent";

fn point() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn child() -> SyndicThreadId {
    SyndicThreadId::from_bytes([180; 16])
}

fn request(fixture: &Fixture, id: u64) -> TranscriptAttachmentRequest {
    let mut request = fixture.request(id);
    request.thread_id = child();
    request
}

fn head(fixture: &Fixture) -> TranscriptViewHeadRecord {
    fixture
        .storage
        .transcript_view_head(&fixture.home, child(), point())
        .unwrap()
        .unwrap()
}

fn build(fixture: &Fixture) -> Option<TranscriptBuildRecord> {
    fixture
        .storage
        .transcript_build(&fixture.home, child(), head(fixture).generation(), point())
        .unwrap()
}

fn quiescent(fixture: &Fixture) {
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

fn retained_count(
    authority: &crate::cas_projection::SelectedTranscriptConstructionAuthority,
) -> usize {
    authority.inspect_retained(|outcomes, _| outcomes.len())
}

fn construct(fixture: &Fixture) {
    let authority = fixture.service.selected_transcript_construction_authority();
    for _ in 0..32 {
        let before = fixture.home.home_revision().unwrap();
        let progress = authority.step(child(), &AtomicBool::new(false)).unwrap();
        let after = fixture.home.home_revision().unwrap();
        assert!(after.get() - before.get() <= 8);
        assert!(retained_count(&authority) <= 8);
        quiescent(fixture);
        if matches!(progress, SelectedTranscriptConstructionProgress::Current) {
            return;
        }
        assert!(matches!(
            progress,
            SelectedTranscriptConstructionProgress::Pending
        ));
    }
    panic!("selected canonical construction did not finish");
}

#[test]
fn inherited_unbound_discussion_is_initially_unpublished_then_canonically_readable_with_context() {
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 1);
    let initial = head(&fixture);
    assert_eq!(initial.generation(), TranscriptGeneration::new(1).unwrap());
    assert_eq!(initial.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(initial.entry_count(), 0);
    assert!(build(&fixture).is_none());
    let binding = fixture
        .storage
        .current_binding(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.binding().state().lifecycle(),
        BindingLifecycle::Unbound
    );
    assert!(binding.binding().state().cas_thread_id().is_none());
    let revision = fixture.home.home_revision().unwrap();
    let unavailable = fixture
        .reader
        .prepare_attachment(request(&fixture, 1), &AtomicBool::new(false))
        .unwrap();
    assert!(matches!(
        unavailable.activation().view_page_response.as_ref(),
        Some(crate::syndic_transcript::TranscriptProviderResponseKind::ViewPage(page))
            if page.records.is_empty() && matches!(page.history_state,
                crate::syndic_transcript::TranscriptProviderHistoryState::Unavailable {
                    reason: crate::syndic_transcript::TranscriptProviderHistoryReason::ProjectionStale,
                    ..
                })
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    drop(unavailable);
    construct(&fixture);
    let current = head(&fixture);
    assert_eq!(current.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(current.entry_count(), 2);
    let prepared = fixture
        .reader
        .prepare_attachment(request(&fixture, 2), &AtomicBool::new(false))
        .unwrap();
    let activation = prepared.activation();
    assert!(activation.view_id.is_some());
    let context = activation.discussion_context.as_ref().unwrap();
    assert_eq!(
        context
            .chunks
            .iter()
            .map(|chunk| chunk.text.as_ref())
            .collect::<String>(),
        CONTEXT
    );
    assert_eq!(context.provenance.insertion_after, 2);
    drop(prepared);
    let revision = fixture.home.home_revision().unwrap();
    construct(&fixture);
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    quiescent(&fixture);
}

#[test]
fn long_inherited_path_yields_at_eight_commands_and_resumes_the_same_build_after_draft_publication()
{
    let fixture = Fixture::new();
    history::create_inherited_discussion(&fixture, 10);
    let authority = fixture.service.selected_transcript_construction_authority();
    let before = fixture.home.home_revision().unwrap();
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)).unwrap(),
        SelectedTranscriptConstructionProgress::Pending
    ));
    let after = fixture.home.home_revision().unwrap();
    assert_eq!(after.get() - before.get(), 8);
    let old = build(&fixture).unwrap();
    assert!(matches!(
        old.phase(),
        TranscriptBuildPhase::Collecting { .. }
    ));
    assert_eq!(head(&fixture).entry_count(), 0);
    assert_eq!(retained_count(&authority), 8);
    let original_thread = fixture
        .storage
        .thread(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    let original_draft = fixture
        .storage
        .current_draft(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    composer_support::seed_published_draft_chunks(&fixture.storage, &fixture.home, child(), 1);
    let updated_thread = fixture
        .storage
        .thread(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    let updated_draft = fixture
        .storage
        .current_draft(&fixture.home, child(), point())
        .unwrap()
        .unwrap();
    assert!(updated_draft.draft().revision() > original_draft.draft().revision());
    assert_eq!(
        original_thread.selected_path_digest(),
        updated_thread.selected_path_digest()
    );
    assert_eq!(build(&fixture).unwrap(), old);
    construct(&fixture);
    assert_eq!(head(&fixture).generation(), old.generation());
    assert_eq!(head(&fixture).entry_count(), 20);
    assert_eq!(
        build(&fixture).unwrap().phase(),
        TranscriptBuildPhase::Complete
    );
    quiescent(&fixture);
}

#[test]
fn preexisting_collecting_and_publishing_builds_resume_without_another_start() {
    for publishing in [false, true] {
        let fixture = Fixture::new();
        history::create_inherited_discussion(&fixture, 2);
        history::start(&fixture);
        if publishing {
            while matches!(
                build(&fixture).unwrap().phase(),
                TranscriptBuildPhase::Collecting { .. }
            ) {
                history::advance(&fixture);
            }
            assert!(matches!(
                build(&fixture).unwrap().phase(),
                TranscriptBuildPhase::Publishing { .. }
            ));
        }
        let generation = head(&fixture).generation();
        construct(&fixture);
        assert_eq!(head(&fixture).generation(), generation);
        assert_eq!(head(&fixture).entry_count(), 4);
        quiescent(&fixture);
    }
}

#[test]
fn unavailable_item_projection_yields_without_constructing_a_projection_or_publishing_partial_history()
 {
    let fixture = Fixture::new();
    let (_, item) = history::create_inherited_discussion(&fixture, 1);
    let old = fixture
        .storage
        .item_projection_head(&fixture.home, item, point())
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::ItemProjectionHead(item))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    let authority = fixture.service.selected_transcript_construction_authority();
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)).unwrap(),
        SelectedTranscriptConstructionProgress::WaitingForProjection
    ));
    assert_eq!(head(&fixture).entry_count(), 0);
    assert_eq!(head(&fixture).lifecycle(), ProjectionLifecycle::Stale);
    let revision = fixture.home.home_revision().unwrap();
    assert!(matches!(
        authority.step(child(), &AtomicBool::new(false)).unwrap(),
        SelectedTranscriptConstructionProgress::WaitingForProjection
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert!(
        fixture
            .storage
            .item_projection_head(&fixture.home, item, point())
            .unwrap()
            .is_none()
    );
    let mut restored = FixtureBatch::new();
    restored
        .put(FixtureRecord::ItemProjectionHead(old))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), restored),
    );
    construct(&fixture);
    quiescent(&fixture);
}

#[path = "selected_construction/custody.rs"]
mod custody;
#[path = "selected_construction/lifecycle.rs"]
mod lifecycle;
#[path = "selected_construction/peer_cooperation.rs"]
mod peer_cooperation;
