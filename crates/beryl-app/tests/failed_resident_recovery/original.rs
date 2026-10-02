use super::host::{fail, retire, save};
use super::{composer, support};
use beryl_app::composer_host::ComposerHostAutosaveCapture;
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1 as Evidence, SyndicStorage, SyndicTimestamp,
};
#[test]
fn original_committed_indeterminate_and_noncommit_remain_independent_of_recovery_save() {
    for cut in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ] {
        original(Some(cut), false);
    }
    original(None, false);
}

#[test]
fn older_known_and_reconciled_autosave_commit_never_certify_newer_edits_saved() {
    original(None, true);
    original(Some(FaultPoint::AfterCommitBeforePersist), true);
}

fn original(cut: Option<FaultPoint>, newer: bool) {
    original_with_recovery_cut(cut, newer, None);
}

#[test]
fn older_original_commit_and_newer_candidate_survive_recovery_save_cuts_and_repeated_qualification()
{
    for original_cut in [None, Some(FaultPoint::AfterCommitBeforePersist)] {
        for recovery_cut in [
            FaultPoint::BeforeCommit,
            FaultPoint::AfterCommitBeforePersist,
        ] {
            original_with_recovery_cut(original_cut, true, Some(recovery_cut));
        }
    }
}

fn original_with_recovery_cut(
    cut: Option<FaultPoint>,
    newer: bool,
    recovery_cut: Option<FaultPoint>,
) {
    let mut fixture = support::host("failed-original-publication", 71);
    let empty = fixture.host.binding().unwrap();
    let first = composer::commit_text(
        &mut fixture.host,
        &fixture.store,
        empty,
        201,
        0,
        0,
        "a",
        1,
        1,
    );
    let timer = fixture.host.autosave_timer().unwrap();
    let ComposerHostAutosaveCapture::Captured(ticket) = fixture
        .host
        .fire_autosave(
            &fixture.store,
            timer,
            fixture.assets.clone(),
            &fixture.seals,
            composer::operation_id(202),
            None,
            SyndicTimestamp::from_unix_millis(202),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("expected captured ordinary autosave");
    };
    let retained_binding = if newer {
        composer::commit_text(
            &mut fixture.host,
            &fixture.store,
            first,
            203,
            1,
            1,
            "b",
            2,
            1,
        )
    } else {
        first
    };
    if let Some(cut) = cut {
        let faults = fixture.faults.clone();
        fixture
            .host
            .test_arm_publication_before_execute_fault(move |_, _| {
                faults.fail_next(cut);
            });
    } else {
        let faults = fixture.faults.clone();
        fixture
            .host
            .test_arm_publication_convergence_read_fault(move |store, _| {
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
                assert!(store.home_revision().is_err());
            });
    }
    let _ = fixture.host.advance_autosave(&fixture.store, ticket);
    if fixture.store.health().state() == HomeHealthState::Healthy {
        fail(&fixture);
    }
    assert_eq!(
        fixture.store.health().state(),
        HomeHealthState::Failed,
        "cut={cut:?} newer={newer}"
    );
    let expected_original = cut != Some(FaultPoint::BeforeCommit);
    let faults = fixture.faults.clone();
    let (_directory, mut candidate, mut storage, mut state, mut retained) = retire(fixture);
    let already_saved = retained
        .qualify_saved(&mut candidate, &storage)
        .unwrap_or_else(|error| panic!("cut={cut:?} newer={newer}: {error}"));
    assert_eq!(retained.original_known_commit(), Some(expected_original));
    assert_eq!(already_saved, expected_original && !newer);
    if let Some(recovery_cut) = recovery_cut {
        assert!(!retained.qualify_saved(&mut candidate, &storage).unwrap());
        let qualified = retained
            .qualified_publication_checkpoint(&mut candidate, &storage)
            .unwrap();
        assert_eq!(qualified.root(), retained_binding.root());
        assert_eq!(qualified.history(), retained_binding.candidate().history());
        assert_eq!(retained.checkpoint(), retained_binding.candidate());
        faults.fail_next(recovery_cut);
        retained
            .publish_candidate(
                &mut candidate,
                &storage,
                &state.assets(),
                composer::operation_id(204),
                SyndicTimestamp::from_unix_millis(204),
                Evidence::UnchangedEmpty,
                CommandCancellation::new(),
            )
            .unwrap();
        let before_reopen_generation = candidate.generation();
        candidate = candidate.abort().recover_same_home().unwrap();
        assert_ne!(candidate.generation(), before_reopen_generation);
        storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
        let committed = recovery_cut != FaultPoint::BeforeCommit;
        assert_eq!(
            retained.qualify_saved(&mut candidate, &storage).unwrap(),
            committed
        );
        assert_eq!(retained.original_known_commit(), Some(true));
        assert_eq!(retained.recovery_known_commit(), Some(committed));
        assert_eq!(
            retained.qualify_saved(&mut candidate, &storage).unwrap(),
            committed
        );
        if committed {
            assert!(retained.retry_publication(&mut candidate).is_err());
        } else {
            retained.retry_publication(&mut candidate).unwrap();
            save(
                &mut retained,
                &mut candidate,
                &storage,
                &state,
                Evidence::UnchangedEmpty,
                205,
            );
        }
        assert!(retained.qualify_saved(&mut candidate, &storage).unwrap());
    }
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    if !already_saved && recovery_cut.is_none() {
        save(
            &mut retained,
            &mut candidate,
            &storage,
            &state,
            Evidence::UnchangedEmpty,
            204,
        );
    } else if recovery_cut.is_none() {
        assert_eq!(retained.recovery_known_commit(), None);
    }
    let host = retained
        .reconstruct(&mut candidate, storage.clone())
        .unwrap();
    let fresh = host.binding().unwrap();
    assert_eq!(fresh.root(), retained_binding.root());
    assert_eq!(
        fresh.candidate().session_id(),
        retained_binding.candidate().session_id()
    );
    support::assert_history_preserved(fresh.history(), retained_binding.history());
    if already_saved {
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
    }
    assert_eq!(retained.original_known_commit(), Some(expected_original));
    let store = candidate.publish().unwrap();
    assert_eq!(
        composer::candidate_text(storage, &store, fresh),
        if newer {
            b"ab".as_slice()
        } else {
            b"a".as_slice()
        }
    );
    drop(host);
    store.close().unwrap();
}
