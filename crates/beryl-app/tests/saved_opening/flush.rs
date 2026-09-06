use beryl_app::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostFlushAdvance,
    ComposerHostFlushCapture, ComposerHostFlushPurpose, ComposerHostFlushState,
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use syndic_storage::{DraftEditorCandidateSessionReadOutcomeV1, SyndicTimestamp};

use super::{base, composer, lifecycle, support::Fixture};

#[test]
fn clean_empty_flush_requires_storage_authentication_and_creates_no_publication() {
    for purpose in [
        ComposerHostFlushPurpose::Submission,
        ComposerHostFlushPurpose::WindowClose,
    ] {
        let mut fixture = Fixture::new("empty-flush", 10);
        let resident = fixture.binding();
        let durable = fixture.current();
        let revision = fixture.store.home_revision().unwrap();
        assert!(!fixture.host.is_dirty());
        assert!(fixture.host.autosave_timer().is_none());
        let (ticket, state) = fixture.begin(purpose);
        assert_eq!(state, ComposerHostFlushState::CaptureRequired);
        assert_eq!(
            fixture.host.flush_state(ticket).unwrap(),
            ComposerHostFlushState::CaptureRequired
        );
        assert_eq!(
            fixture.host.advance_flush(&fixture.store, ticket).unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
        );
        let capture = fixture.capture(ticket, 1);
        if purpose == ComposerHostFlushPurpose::Submission {
            assert_eq!(capture, ComposerHostFlushCapture::Satisfied(purpose));
            assert_eq!(fixture.host.lifecycle_diagnostics().barriers(), 0);
        } else {
            assert_eq!(
                capture,
                ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
            );
            assert_eq!(
                fixture.host.advance_flush(&fixture.store, ticket).unwrap(),
                ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CloseReady)
            );
            assert!(fixture.host.release_window_close(ticket).unwrap());
        }
        assert_eq!(fixture.binding(), resident);
        assert_eq!(fixture.current(), durable);
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
        assert_eq!(fixture.host.publication_custody_count(), 0);
        assert!(fixture.host.autosave_timer().is_none());
    }
}

#[test]
fn reopened_openings_close_through_ordinary_disposal_without_a_selector_publication() {
    for text in ["", "saved checkpoint"] {
        let mut fixture = Fixture::reopened("reopened-close", 30, text);
        let resident = fixture.binding();
        let durable = fixture.current();
        let revision = fixture.store.home_revision().unwrap();
        assert!(!fixture.host.is_dirty());
        assert!(fixture.host.autosave_timer().is_none());
        let (ticket, state) = fixture.begin(ComposerHostFlushPurpose::WindowClose);
        assert_eq!(state, ComposerHostFlushState::CaptureRequired);
        assert_eq!(
            fixture.capture(ticket, 10),
            ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
        );
        assert_eq!(fixture.binding(), resident);
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
        assert_eq!(fixture.host.publication_custody_count(), 0);
        assert_eq!(
            fixture
                .host
                .authorize_window_close_disposal(&fixture.store, ticket)
                .unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
        );
        assert_eq!(
            fixture
                .host
                .capture_flush_disposal(
                    &fixture.store,
                    ticket,
                    composer::operation_id(11),
                    &CommandCancellation::new()
                )
                .unwrap(),
            ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
        );
        assert_eq!(
            fixture.host.advance_flush(&fixture.store, ticket).unwrap(),
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::WindowClose)
        );
        assert!(fixture.host.binding().is_none());
        let DraftEditorCandidateSessionReadOutcomeV1::Disposed(terminal) = fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                resident.candidate().draft_id(),
                resident.candidate().session_id(),
            )
            .unwrap()
        else {
            panic!("ordinary opening disposal did not retain a readable terminal session");
        };
        assert_eq!(terminal.newest_root(), resident.root());
        assert_eq!(terminal.newest_history(), durable.draft().history());
        assert_eq!(
            terminal.disposal_operation_id(),
            Some(composer::operation_id(11))
        );
        assert_eq!(fixture.current(), durable);
        assert_eq!(fixture.host.publication_custody_count(), 0);
    }
}

#[test]
fn a_later_edit_requires_real_publication_and_new_saved_authentication() {
    let mut fixture = Fixture::reopened("later-edit", 50, "saved");
    let opening = fixture.binding();
    let (close, _) = fixture.begin(ComposerHostFlushPurpose::WindowClose);
    assert_eq!(
        fixture.capture(close, 10),
        ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
    );
    assert!(fixture.host.release_window_close(close).unwrap());
    let (flush, state) = fixture.begin(ComposerHostFlushPurpose::Submission);
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    let edited = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        opening,
        3,
        5,
        5,
        "!",
        6,
        1,
    );
    assert!(fixture.host.is_dirty());
    assert!(fixture.host.autosave_timer().is_some());
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert!(matches!(
        fixture.capture(flush, 11),
        ComposerHostFlushCapture::Captured(_)
    ));
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_eq!(fixture.binding().root(), edited.root());
    assert!(!fixture.host.is_dirty());
    assert_eq!(
        fixture.capture(flush, 12),
        ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    assert_eq!(fixture.current().draft().piece_root(), edited.root());
}

#[test]
fn stale_selector_and_missing_history_cannot_satisfy_a_locally_clean_barrier() {
    for missing_history in [false, true] {
        let mut fixture = Fixture::new("invalid-clean", 70);
        let resident = fixture.binding();
        if missing_history {
            base::committed(base::execute(
                &fixture.store,
                syndic_storage::test_faults::delete_draft_edit_history_frontier(
                    &fixture.store,
                    fixture.storage.clone(),
                    resident.history().key(),
                ),
            ));
        } else {
            let selector =
                lifecycle::selector(fixture.storage.clone(), &fixture.store, fixture.thread);
            let (mut competitor, empty) = composer::activated(
                fixture.storage.clone(),
                &fixture.store,
                fixture.thread,
                80,
                81,
            );
            let changed = composer::commit_text(
                &mut competitor,
                &fixture.store,
                empty,
                1,
                0,
                0,
                "other",
                5,
                1,
            );
            lifecycle::publish_binding(
                &fixture.store,
                fixture.storage.clone(),
                selector,
                changed,
                82,
                82,
            );
        }
        let durable = fixture.current();
        let revision = fixture.store.home_revision().unwrap();
        assert!(!fixture.host.is_dirty());
        let (ticket, state) = fixture.begin(ComposerHostFlushPurpose::WindowClose);
        assert_eq!(state, ComposerHostFlushState::CaptureRequired);
        assert_eq!(
            fixture.host.advance_flush(&fixture.store, ticket).unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
        );
        assert!(matches!(
            fixture.capture(ticket, 83),
            ComposerHostFlushCapture::Unsatisfied(_)
        ));
        assert_eq!(fixture.binding(), resident);
        assert_eq!(fixture.current(), durable);
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
        assert_eq!(fixture.host.publication_custody_count(), 0);
        assert!(matches!(
            fixture
                .host
                .authorize_window_close_disposal(&fixture.store, ticket),
            Ok(ComposerHostFlushAdvance::Stale) | Err(_)
        ));
    }
}

#[test]
fn a_pending_real_publication_retains_custody_and_drains_its_dirty_successor() {
    let mut fixture = Fixture::new("pending-publication", 90);
    let opening = fixture.binding();
    let first = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        opening,
        1,
        0,
        0,
        "a",
        1,
        1,
    );
    let timer = fixture.host.autosave_timer().unwrap();
    let publication = match fixture
        .host
        .fire_autosave(
            &fixture.store,
            timer,
            fixture.assets.clone(),
            &fixture.seals,
            composer::operation_id(10),
            None,
            SyndicTimestamp::from_unix_millis(10),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostAutosaveCapture::Captured(ticket) => ticket,
        other => panic!("autosave was not captured: {other:?}"),
    };
    let latest =
        composer::commit_text(&mut fixture.host, &fixture.store, first, 2, 1, 1, "b", 2, 1);
    let faults = fixture.faults.clone();
    fixture
        .host
        .test_arm_publication_before_execute_fault(move |_, _| {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist)
        });
    assert_eq!(
        fixture
            .host
            .advance_autosave(&fixture.store, publication)
            .unwrap(),
        ComposerHostAutosaveAdvance::ReconciliationPending
    );
    assert_eq!(fixture.host.publication_custody_count(), 1);
    let (flush, state) = fixture.begin(ComposerHostFlushPurpose::Submission);
    assert_eq!(state, ComposerHostFlushState::PublicationPending);
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_eq!(fixture.binding().root(), latest.root());
    assert!(fixture.host.is_dirty());
    assert_eq!(fixture.host.publication_custody_count(), 0);
    assert!(matches!(
        fixture.capture(flush, 11),
        ComposerHostFlushCapture::Captured(_)
    ));
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_eq!(
        fixture.capture(flush, 12),
        ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    assert_eq!(fixture.current().draft().piece_root(), latest.root());
    assert_eq!(fixture.host.publication_custody_count(), 0);
    assert!(fixture.host.autosave_timer().is_none());
}
