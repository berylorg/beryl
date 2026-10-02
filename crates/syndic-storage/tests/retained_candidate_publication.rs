#![cfg(feature = "test-faults")]

#[path = "draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "abandon_fresh_candidate/shared.rs"]
#[allow(dead_code)]
mod publication_support;

#[path = "retained_candidate_publication/markers.rs"]
#[allow(dead_code, unused_imports)]
mod markers;
#[path = "retained_candidate_publication/outcomes.rs"]
mod outcomes;
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

use beryl_home_store::{CommandCancellation, HomeCandidateRecoveryAccess};
use edit_support::commit_edit;
use publication_support::*;
use support::*;
use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1, DraftHistoricalRootSelectionV1, DraftMutationBeginV1,
    DraftMutationOperationIdV1, DraftMutationStagingIdentityV1,
    PreparedDraftEditorCandidatePublicationV1,
};

fn capture_request(
    selected: DraftEditorCurrentSelectorV1,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftEditorCandidatePublicationSourceCaptureRequestV1 {
    DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
        selected,
        DraftEditorCandidateActivationBindingV1::from_head(session),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        SyndicTimestamp::from_unix_millis(3),
    )
}

fn prepare_candidate(
    storage: &SyndicStorage,
    access: &HomeCandidateRecoveryAccess<'_>,
    request: DraftEditorCandidatePublicationSourceCaptureRequestV1,
) -> PreparedDraftEditorCandidatePublicationV1 {
    let source = storage
        .capture_draft_editor_candidate_publication_source_candidate(access, request)
        .unwrap();
    storage
        .prepare_draft_editor_candidate_publication_candidate(
            access,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap()
}

fn candidate_command(
    storage: &SyndicStorage,
    access: &HomeCandidateRecoveryAccess<'_>,
    prepared: &PreparedDraftEditorCandidatePublicationV1,
) -> HomeCommand {
    let contribution = storage
        .publish_draft_editor_candidate_candidate(
            access,
            storage.revision_candidate(access).unwrap(),
            prepared.clone(),
        )
        .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    command
}

#[test]
fn original_publication_outcomes_are_settled_before_exact_fresh_candidate_save() {
    for (cut, committed_at_cut) in [
        (FaultPoint::BeforeCommit, false),
        (FaultPoint::AfterCommitBeforePersist, true),
        (FaultPoint::AfterPersist, true),
        (FaultPoint::BeforeVerification, true),
    ] {
        let (_home, store, storage, faults, thread) =
            fault_fixture("retained-original", 10, 65_536);
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 12, 13);
        let edit = commit_edit(&storage, &store, &opened, 14, "exact retained text");
        let candidate = edit.adopted_session();
        let original = prepare_candidate_publication(&storage, &store, &selected, candidate, 15);
        faults.fail_next(cut);
        let outcome = execute(
            &store,
            storage.publish_draft_editor_candidate(
                storage.revision(&store).unwrap(),
                original.clone(),
            ),
        );
        if store.health().state() == HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let original_result = fresh
            .reconcile_draft_editor_candidate_publication_candidate(&access, &original, outcome);
        let saved_selector = if committed_at_cut {
            let DraftEditorCandidatePublicationOutcomeV1::Published(selector, pair) =
                original_result.unwrap()
            else {
                panic!("original commit not proven")
            };
            assert_eq!(pair.root(), candidate.newest_root());
            let revision = access.home_revision().unwrap();
            let correspondence = fresh
                .qualify_published_draft_editor_candidate_candidate(
                    &access,
                    DraftEditorCandidateActivationBindingV1::from_head(candidate),
                    &original,
                )
                .unwrap();
            assert_eq!(correspondence.selector(), selector);
            assert_eq!(correspondence.candidate().root(), candidate.newest_root());
            assert_eq!(correspondence.home_id(), access.home_id());
            assert_eq!(correspondence.generation(), access.generation());
            assert_eq!(access.home_revision().unwrap(), revision);
            selector
        } else {
            assert!(matches!(
                original_result,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
            assert!(
                !fresh
                    .draft_editor_candidate_is_saved_candidate(
                        &access,
                        DraftEditorCandidateActivationBindingV1::from_head(candidate),
                        selector(&selected)
                    )
                    .unwrap()
            );
            let prepared = prepare_candidate(
                &fresh,
                &access,
                capture_request(selector(&selected), candidate, 16),
            );
            let outcome = access.execute(candidate_command(&fresh, &access, &prepared));
            let DraftEditorCandidatePublicationOutcomeV1::Published(selector, pair) = fresh
                .reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome)
                .unwrap()
            else {
                panic!("fresh save not proven")
            };
            assert_eq!(pair.root(), candidate.newest_root());
            assert_ne!(pair.history(), candidate.newest_history());
            let correspondence = fresh
                .qualify_published_draft_editor_candidate_candidate(
                    &access,
                    DraftEditorCandidateActivationBindingV1::from_head(candidate),
                    &prepared,
                )
                .unwrap();
            assert_eq!(correspondence.selector(), selector);
            assert_eq!(correspondence.candidate().history(), pair.history());
            let revision = access.home_revision().unwrap();
            let replay = access.execute(candidate_command(&fresh, &access, &prepared));
            assert!(matches!(
                fresh
                    .reconcile_draft_editor_candidate_publication_candidate(
                        &access, &prepared, replay
                    )
                    .unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::ExactReplay(_)
            ));
            assert_eq!(access.home_revision().unwrap(), revision);
            selector
        };
        let store = recovery.publish().unwrap();
        let live = head(&fresh, &store, candidate);
        assert_eq!(live.newest_root(), candidate.newest_root());
        assert_eq!(live.newest_history(), live.published_history());
        assert_eq!(
            live.newest_candidate_generation(),
            candidate.newest_candidate_generation()
        );
        assert_eq!(
            live.newest_history().availability(),
            candidate.newest_history().availability()
        );
        assert_eq!(selector(&current(&fresh, &store, thread)), saved_selector);
    }
}

#[test]
fn candidate_capture_rejects_stale_foreign_disposed_and_pending_authority() {
    for invalid in 0..6 {
        let (_home, store, storage, faults, thread) = fault_fixture("retained-invalid", 30, 65_536);
        eprintln!("retained invalid case {invalid}: {}", _home.0.display());
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 32, 33);
        let opened = if invalid == 4 {
            commit_edit(&storage, &store, &opened, 37, "required stored history")
                .adopted_session()
                .clone()
        } else {
            opened
        };
        let request = capture_request(selector(&selected), &opened, 34);
        let old_source = storage
            .capture_draft_editor_candidate_publication_source(&store, request)
            .unwrap();
        match invalid {
            0 => {
                commit_edit(&storage, &store, &opened, 35, "advanced");
            }
            1 => {
                let edit = commit_edit(&storage, &store, &opened, 35, "changed selector");
                publish_candidate(&storage, &store, &selected, edit.adopted_session(), 36);
            }
            2 => {
                let dispose = storage
                    .prepare_dispose_draft_editor_candidate_session(
                        &store,
                        syndic_storage::DraftEditorCandidateSessionDisposeRequestV1::new(
                            opened.draft_id(),
                            opened.session_id(),
                            DraftPieceOperationIdV1::from_bytes([35; 16]),
                            opened.session_generation(),
                            syndic_storage::DraftRootHistoryPairV1::new(
                                opened.published_root(),
                                opened.published_history(),
                            ),
                        ),
                    )
                    .unwrap();
                committed(execute(
                    &store,
                    storage.dispose_draft_editor_candidate_session(
                        storage.revision(&store).unwrap(),
                        dispose,
                    ),
                ));
            }
            3 => {
                let edit = transaction(&storage, &store, &opened, 35, "pending", point(7));
                committed(execute(
                    &store,
                    storage
                        .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared),
                ));
            }
            4 => {
                committed(execute(
                    &store,
                    delete_draft_edit_history_frontier(
                        &store,
                        storage.clone(),
                        opened.newest_history().key(),
                    ),
                ));
            }
            _ => {
                let identity = DraftMutationStagingIdentityV1::new(
                    opened.draft_id(),
                    opened.session_id(),
                    DraftMutationOperationIdV1::from_bytes([35; 16]),
                );
                let request = DraftMutationBeginV1::new(
                    identity,
                    opened.session_generation(),
                    opened.newest_candidate_generation(),
                    opened.newest_root(),
                    opened.newest_history(),
                    opened.logical_extent(),
                    point(0),
                    point(0),
                    point(0),
                    point(0),
                    point(0),
                    0,
                    0,
                );
                let begin = storage
                    .prepare_draft_mutation_staging_begin(request, &opened)
                    .unwrap();
                committed(execute(
                    &store,
                    storage
                        .draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
                ));
            }
        }
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let revision = access.home_revision().unwrap();
        assert!(
            storage
                .capture_draft_editor_candidate_publication_source_candidate(&access, request)
                .is_err()
        );
        assert!(
            fresh
                .capture_draft_editor_candidate_publication_source_candidate(&access, request)
                .is_err()
        );
        assert!(
            fresh
                .prepare_draft_editor_candidate_publication_candidate(
                    &access,
                    old_source,
                    DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty
                )
                .is_err()
        );
        let (_foreign_home, _foreign_store, foreign, _) = fixture("retained-foreign", 40, 65_536);
        assert!(
            foreign
                .capture_draft_editor_candidate_publication_source_candidate(&access, request)
                .is_err()
        );
        assert_eq!(access.home_revision().unwrap(), revision);
        recovery.abort().close().unwrap();
    }
}

#[test]
fn candidate_preparation_keeps_its_captured_checkpoint_after_historical_advance() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("retained-captured-history", 50, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 52, 53);
    let edit = commit_edit(&storage, &store, &opened, 54, "captured text");
    let captured = edit.adopted_session();
    let intent = DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(captured),
        DraftPieceOperationIdV1::from_bytes([55; 16]),
        DraftHistoricalRootDirectionV1::Undo,
    );
    let DraftHistoricalRootSelectionV1::Prepared(undo) = storage
        .prepare_draft_historical_root_selection(&store, intent)
        .unwrap()
    else {
        panic!("undo unavailable")
    };
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let source = fresh
        .capture_draft_editor_candidate_publication_source_candidate(
            &access,
            capture_request(selector(&selected), captured, 56),
        )
        .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(fresh.adopt_draft_historical_root(fresh.revision_candidate(&access).unwrap(), undo))
        .unwrap();
    committed(access.execute(command));
    let prepared = fresh
        .prepare_draft_editor_candidate_publication_candidate(
            &access,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    assert_eq!(
        prepared.request().candidate().root(),
        captured.newest_root()
    );
    let outcome = access.execute(candidate_command(&fresh, &access, &prepared));
    assert!(matches!(
        fresh
            .reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    assert!(
        fresh
            .qualify_published_draft_editor_candidate_candidate(
                &access,
                DraftEditorCandidateActivationBindingV1::from_head(captured),
                &prepared
            )
            .is_err()
    );
    let store = recovery.publish().unwrap();
    let live = head(&fresh, &store, captured);
    assert_eq!(live.newest_root(), opened.newest_root());
    assert_ne!(live.newest_history(), captured.newest_history());
    assert_eq!(live.published_root(), captured.newest_root());
    assert!(
        !fresh
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&live),
                selector(&current(&fresh, &store, thread))
            )
            .unwrap()
    );
}

#[test]
fn cancelled_candidate_save_retains_unsaved_checkpoint_and_revision_fence() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("retained-cancellation", 70, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 72, 73);
    let edit = commit_edit(&storage, &store, &opened, 74, "retained on cancellation");
    let captured = edit.adopted_session();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let prepared = prepare_candidate(
        &fresh,
        &access,
        capture_request(selector(&selected), captured, 75),
    );
    let before = access.home_revision().unwrap();
    let revision = fresh.revision_candidate(&access).unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let outcome = access
        .execute(candidate_command(&fresh, &access, &prepared).with_cancellation(cancellation));
    assert!(matches!(
        fresh.reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome),
        Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
    ));
    assert_eq!(access.home_revision().unwrap(), before);
    assert!(
        !fresh
            .draft_editor_candidate_is_saved_candidate(
                &access,
                DraftEditorCandidateActivationBindingV1::from_head(captured),
                selector(&selected)
            )
            .unwrap()
    );
    committed(access.execute(candidate_command(&fresh, &access, &prepared)));
    assert!(
        fresh
            .publish_draft_editor_candidate_candidate(&access, revision, prepared)
            .is_err()
    );
    let store = recovery.publish().unwrap();
    let live = head(&fresh, &store, captured);
    assert_eq!(live.newest_root(), captured.newest_root());
    assert_eq!(live.newest_history(), live.published_history());
}

#[test]
fn saved_inherited_openings_return_authenticated_correspondence_without_mutation() {
    for inherited in [false, true] {
        let (_home, store, storage, faults, thread) = fault_fixture("retained-saved", 130, 65_536);
        let initial = current(&storage, &store, thread);
        if inherited {
            let first = open_session(&storage, &store, &initial, 132, 133);
            let edit = commit_edit(&storage, &store, &first, 134, "inherited generation");
            publish_candidate(&storage, &store, &initial, edit.adopted_session(), 135);
        }
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 136, 137);
        assert_eq!(opened.newest_candidate_generation() > 0, inherited);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let before = access.home_revision().unwrap();
        let correspondence = fresh
            .qualify_saved_draft_editor_candidate_candidate(
                &access,
                DraftEditorCandidateActivationBindingV1::from_head(&opened),
                selector(&selected),
            )
            .unwrap();
        assert_eq!(
            correspondence.candidate(),
            DraftEditorCandidateActivationBindingV1::from_head(&opened)
        );
        assert_eq!(correspondence.selector(), selector(&selected));
        assert_eq!(access.home_revision().unwrap(), before);
        recovery.publish().unwrap().close().unwrap();
    }
}
