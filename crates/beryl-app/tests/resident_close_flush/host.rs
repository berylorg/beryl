use beryl_app::composer_host::{
    ComposerHostAutosaveAdvance, ComposerHostAutosaveCapture, ComposerHostError,
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
    ComposerHostFlushFailure, ComposerHostFlushPurpose, ComposerHostFlushState,
    ComposerHostFlushTicket, ComposerHostPublicationTicket,
};
use beryl_home_store::{CommandCancellation, test_faults::FaultPoint};
use syndic_storage::SyndicTimestamp;

use super::{
    composer,
    support::{self, Host},
};

#[test]
fn clean_and_dirty_close_keep_exact_editor_until_success_and_reject_stale_settlement() {
    for dirty in [false, true] {
        let mut fixture = support::host("exact-close", 101 + u8::from(dirty) * 4);
        let initial = fixture.host.binding().unwrap();
        if dirty {
            composer::commit_text(
                &mut fixture.host,
                &fixture.store,
                initial,
                1,
                0,
                0,
                "a",
                1,
                1,
            );
        }
        let resident = fixture.host.binding().unwrap();
        let close = begin_close(&mut fixture);
        for _ in 0..8 {
            assert!(matches!(
                fixture.host.begin_flush(ComposerHostFlushPurpose::WindowClose).unwrap(),
                ComposerHostFlushAdmission::Joined { ticket, .. } if ticket == close
            ));
        }
        for purpose in [
            ComposerHostFlushPurpose::Submission,
            ComposerHostFlushPurpose::ThreadSwitch,
            ComposerHostFlushPurpose::ApplicationExit,
            ComposerHostFlushPurpose::Release,
        ] {
            assert!(fixture.host.begin_flush(purpose).is_err());
        }
        assert!(matches!(
            composer::begin_text(&mut fixture.host, &fixture.store, resident, 2, 0),
            Err(ComposerHostError::LifecycleBlocked)
        ));
        ready(&mut fixture, close, 10);
        let saved = fixture.host.binding().unwrap();
        assert_eq!(saved.host_generation(), resident.host_generation());
        assert_eq!(
            saved.candidate().session_id(),
            resident.candidate().session_id()
        );
        assert_eq!(saved.root(), resident.root());
        support::assert_history_preserved(saved.history(), resident.history());
        assert_eq!(fixture.host.lifecycle_diagnostics().barriers(), 1);
        assert!(!fixture.host.is_dirty());
        assert!(matches!(
            composer::begin_text(&mut fixture.host, &fixture.store, saved, 3, 0),
            Err(ComposerHostError::LifecycleBlocked)
        ));

        assert!(fixture.host.release_window_close(close).unwrap());
        assert_eq!(fixture.host.binding(), Some(saved));
        assert!(!fixture.host.release_window_close(close).unwrap());
        let end = saved.logical_extent().logical_utf8_bytes();
        let edited = composer::commit_text(
            &mut fixture.host,
            &fixture.store,
            saved,
            4,
            end,
            end,
            "b",
            end + 1,
            1,
        );
        let replacement_close = begin_close(&mut fixture);
        assert_ne!(replacement_close, close);
        assert!(!fixture.host.release_window_close(close).unwrap());
        assert_eq!(
            fixture
                .host
                .authorize_window_close_disposal(&fixture.store, close)
                .unwrap(),
            ComposerHostFlushAdvance::Stale
        );
        assert_eq!(fixture.host.binding(), Some(edited));
        ready(&mut fixture, replacement_close, 11);
        assert_eq!(
            fixture
                .host
                .authorize_window_close_disposal(&fixture.store, replacement_close)
                .unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
        );
        assert!(matches!(
            fixture
                .host
                .capture_flush_disposal(
                    &fixture.store,
                    replacement_close,
                    composer::operation_id(12),
                    &CommandCancellation::new()
                )
                .unwrap(),
            ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
        ));
        assert_eq!(
            fixture
                .host
                .advance_flush(&fixture.store, replacement_close)
                .unwrap(),
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::WindowClose)
        );
        assert!(fixture.host.binding().is_none());
        assert_eq!(fixture.host.lifecycle_diagnostics().barriers(), 0);
        assert_eq!(fixture.host.publication_custody_count(), 0);
        let reopened =
            composer::reactivate(&mut fixture.host, &fixture.store, fixture.thread, 120, 121);
        assert!(
            !fixture
                .host
                .release_window_close(replacement_close)
                .unwrap()
        );
        assert_eq!(
            fixture
                .host
                .authorize_window_close_disposal(&fixture.store, replacement_close)
                .unwrap(),
            ComposerHostFlushAdvance::Stale
        );
        assert_eq!(fixture.host.binding(), Some(reopened));
    }
}

#[test]
fn close_joins_an_older_save_and_drains_the_already_admitted_edit() {
    let mut fixture = support::host("close-autosave", 131);
    let initial = fixture.host.binding().unwrap();
    let first = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        initial,
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
        other => panic!("expected captured autosave: {other:?}"),
    };
    composer::begin_text(&mut fixture.host, &fixture.store, first, 2, 1).unwrap();
    let close = begin_close(&mut fixture);
    let latest = super::mutation::complete_admitted_append(&mut fixture, first, 2, "b");
    assert_eq!(
        fixture.host.flush_state(close).unwrap(),
        ComposerHostFlushState::PublicationPending
    );
    assert!(
        fixture
            .host
            .authorize_window_close_disposal(&fixture.store, close)
            .is_err()
    );
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, close).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_eq!(fixture.host.binding().unwrap().root(), latest.root());
    support::assert_history_preserved(fixture.host.binding().unwrap().history(), latest.history());
    assert!(fixture.host.is_dirty());
    assert_eq!(
        fixture
            .host
            .advance_autosave(&fixture.store, publication)
            .unwrap(),
        ComposerHostAutosaveAdvance::Stale
    );
    ready(&mut fixture, close, 11);
    assert_eq!(fixture.host.binding().unwrap().root(), latest.root());
    support::assert_history_preserved(fixture.host.binding().unwrap().history(), latest.history());
    assert_eq!(fixture.host.lifecycle_diagnostics().barriers(), 1);
    assert!(fixture.host.release_window_close(close).unwrap());
}

#[test]
fn cancelled_close_capture_preserves_dirty_editor_and_requires_new_explicit_attempt() {
    let mut fixture = support::host("close-cancelled", 141);
    let initial = fixture.host.binding().unwrap();
    let dirty = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        initial,
        1,
        0,
        0,
        "a",
        1,
        1,
    );
    let close = begin_close(&mut fixture);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        fixture
            .host
            .capture_flush_publication(
                &fixture.store,
                close,
                fixture.assets.clone(),
                &fixture.seals,
                composer::operation_id(10),
                None,
                SyndicTimestamp::from_unix_millis(10),
                &cancellation
            )
            .unwrap(),
        ComposerHostFlushCapture::Unsatisfied(ComposerHostFlushFailure::Cancelled)
    );
    assert_eq!(fixture.host.binding(), Some(dirty));
    assert!(fixture.host.is_dirty());
    assert_eq!(fixture.host.lifecycle_diagnostics().barriers(), 0);
    assert!(fixture.host.release_window_close(close).unwrap());
    assert!(!fixture.host.release_window_close(close).unwrap());
    let retry = begin_close(&mut fixture);
    assert_ne!(retry, close);
    ready(&mut fixture, retry, 11);
    assert!(fixture.host.release_window_close(retry).unwrap());
}

#[test]
fn close_reconciliation_stays_unsatisfied_and_release_preserves_exact_publication_custody() {
    let mut fixture = support::host("close-reconciliation", 151);
    let initial = fixture.host.binding().unwrap();
    let dirty = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        initial,
        1,
        0,
        0,
        "a",
        1,
        1,
    );
    let close = begin_close(&mut fixture);
    let publication = capture(&mut fixture, close, 10);
    let faults = fixture.faults.clone();
    fixture
        .host
        .test_arm_publication_before_execute_fault(move |_, _| {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        });
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, close).unwrap(),
        ComposerHostFlushAdvance::ReconciliationPending
    );
    assert_eq!(fixture.host.binding().unwrap().root(), dirty.root());
    support::assert_history_preserved(fixture.host.binding().unwrap().history(), dirty.history());
    assert_eq!(fixture.host.publication_custody_count(), 1);
    assert!(
        fixture
            .host
            .authorize_window_close_disposal(&fixture.store, close)
            .is_err()
    );
    assert!(fixture.host.release_window_close(close).unwrap());
    assert_eq!(fixture.host.publication_custody_count(), 1);
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, close).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(
        fixture
            .host
            .advance_autosave(&fixture.store, publication)
            .unwrap(),
        ComposerHostAutosaveAdvance::Saved {
            dirty_successor: false
        }
    );
    assert_eq!(fixture.host.publication_custody_count(), 0);
    assert_eq!(fixture.host.binding().unwrap().root(), dirty.root());
    support::assert_history_preserved(fixture.host.binding().unwrap().history(), dirty.history());
    assert!(!fixture.host.is_dirty());
}

fn begin_close(fixture: &mut Host) -> ComposerHostFlushTicket {
    match fixture
        .host
        .begin_flush(ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. } => ticket,
        other => panic!("expected new close attempt: {other:?}"),
    }
}

fn capture(
    fixture: &mut Host,
    close: ComposerHostFlushTicket,
    operation: u64,
) -> ComposerHostPublicationTicket {
    match fixture
        .host
        .capture_flush_publication(
            &fixture.store,
            close,
            fixture.assets.clone(),
            &fixture.seals,
            composer::operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(operation),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(ticket) => ticket,
        other => panic!("expected close publication for operation {operation}: {other:?}"),
    }
}

fn ready(fixture: &mut Host, close: ComposerHostFlushTicket, operation: u64) {
    if fixture.host.flush_state(close).unwrap() == ComposerHostFlushState::CaptureRequired {
        capture(fixture, close, operation);
    }
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, close).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CloseReady)
    );
    assert_eq!(
        fixture.host.flush_state(close).unwrap(),
        ComposerHostFlushState::CloseReady
    );
}
