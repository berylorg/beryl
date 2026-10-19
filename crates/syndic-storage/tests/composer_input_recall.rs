#![cfg(feature = "test-faults")]

#[path = "draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "submission_disposal/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "abandon_fresh_candidate/shared.rs"]
#[allow(dead_code)]
mod publication_support;
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

use fixture::Fixture;
use support::*;
use syndic_storage::{
    ComposerAcceptedInputReferenceV1, DraftComposerRecallAdvancePreparationV1,
    DraftComposerRecallCommandOutcomeV1, DraftComposerRecallErrorV1, DraftComposerRecallRequestV1,
    DraftPieceRootReferenceV1, PreparedDraftComposerRecallV1,
    PreparedDraftComposerRootReplacementV1,
};

fn operation(value: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([value; 16])
}
fn text(storage: &SyndicStorage, store: &HomeStore, root: DraftPieceRootReferenceV1) -> String {
    let mut bytes = Vec::new();
    let mut offset = 0;
    while offset < root.summary().logical_utf8_bytes() {
        let page = storage
            .draft_piece_text_demand(store, root, DraftPieceTextDemandV1::Forward(offset), 65_536)
            .unwrap();
        assert!(page.bytes().len() <= 65_536);
        assert!(page.end() > offset);
        offset = page.end();
        bytes.extend_from_slice(page.bytes());
    }
    String::from_utf8(bytes).unwrap()
}
fn successor(f: &Fixture, content: &str) -> DraftEditorCandidateSessionV1 {
    let durable = current(&f.storage, &f.store, f.thread);
    let session = open_session(&f.storage, &f.store, &durable, 70, 71);
    if content.is_empty() {
        session
    } else {
        edit_support::commit_edit(&f.storage, &f.store, &session, 72, content)
            .adopted_session()
            .clone()
    }
}
fn prepare(
    f: &Fixture,
    session: &DraftEditorCandidateSessionV1,
    source: ComposerAcceptedInputReferenceV1,
    id: u8,
) -> PreparedDraftComposerRecallV1 {
    f.storage
        .prepare_composer_recall(
            &f.store,
            DraftComposerRecallRequestV1::new(
                DraftEditorCandidateActivationBindingV1::from_head(session),
                operation(id),
                source,
                point(session.logical_extent().logical_utf8_bytes()),
                point(0),
            ),
        )
        .unwrap()
}
fn complete(
    f: &Fixture,
    p: &PreparedDraftComposerRecallV1,
) -> PreparedDraftComposerRootReplacementV1 {
    committed(execute(
        &f.store,
        f.storage
            .begin_composer_recall(f.storage.revision(&f.store).unwrap(), p.clone()),
    ));
    let (build_bytes, progress_bytes) =
        syndic_storage::test_faults::composer_recall_record_sizes(&f.store, &f.storage, p.key());
    assert!(build_bytes <= 8_192, "begin build bytes {build_bytes}");
    assert!(
        progress_bytes <= 8_192,
        "begin progress bytes {progress_bytes}"
    );
    let mut steps = 0;
    loop {
        steps += 1;
        assert!(steps < 16_384);
        match f.storage.prepare_composer_recall_step(&f.store, p).unwrap() {
            DraftComposerRecallAdvancePreparationV1::Ready(step) => committed(execute(
                &f.store,
                f.storage
                    .advance_composer_recall(f.storage.revision(&f.store).unwrap(), step),
            )),
            DraftComposerRecallAdvancePreparationV1::Complete(_) => break,
            DraftComposerRecallAdvancePreparationV1::Image(_) => {
                panic!("text fixture yielded image")
            }
        }
    }
    f.storage
        .prepare_composer_recall_commit(&f.store, p)
        .unwrap()
}
fn adopt(
    f: &Fixture,
    p: &PreparedDraftComposerRootReplacementV1,
) -> DraftEditorCandidateActivationBindingV1 {
    let outcome = execute(
        &f.store,
        f.storage
            .commit_composer_root_replacement(f.storage.revision(&f.store).unwrap(), p.clone()),
    );
    match f
        .storage
        .reconcile_composer_root_replacement(&f.store, p, outcome)
        .unwrap()
    {
        DraftComposerRecallCommandOutcomeV1::Adopted(candidate) => candidate,
        other => panic!("unexpected outcome {other:?}"),
    }
}
fn materialize(
    f: &Fixture,
    root: DraftPieceRootReferenceV1,
    id: u8,
) -> syndic_storage::DraftComposerMaterializationRecordV1 {
    use syndic_storage::{
        DraftComposerBuildKeyV1, DraftComposerFormatV1, DraftComposerMaterializationOperationIdV1,
        DraftComposerMaterializationStatusV1,
    };
    let key = DraftComposerBuildKeyV1::new(
        root,
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([id; 16]),
    );
    committed(execute(
        &f.store,
        f.storage
            .begin_draft_composer_materialization(f.storage.revision(&f.store).unwrap(), key),
    ));
    for _ in 0..16_384 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = f
            .storage
            .draft_composer_materialization_status(&f.store, key)
            .unwrap()
        {
            return mapping;
        }
        let step = f
            .storage
            .prepare_draft_composer_materialization_step(&f.store, key)
            .unwrap()
            .unwrap();
        assert!(step.input_payload_bytes() <= syndic_storage::DRAFT_COMPOSER_INPUT_MAX_BYTES);
        committed(execute(
            &f.store,
            f.storage.advance_draft_composer_materialization(
                f.storage.revision(&f.store).unwrap(),
                step,
            ),
        ));
    }
    panic!("bounded materialization did not finish")
}

#[test]
fn large_unicode_recall_streams_and_restores_checkpoint_after_undo_floor_eviction() {
    let mut f = Fixture::new("recall-large", 90, true);
    let added = "α🌳\n".repeat(35_000);
    let mut source_head = f.source.clone();
    for (index, bytes) in added.as_bytes().chunks(56_000).rev().enumerate() {
        let chunk = std::str::from_utf8(bytes).unwrap();
        let source_edit =
            edit_support::commit_edit(&f.storage, &f.store, &source_head, 112 + index as u8, chunk);
        source_head = source_edit.adopted_session().clone();
    }
    let prior = current(&f.storage, &f.store, f.thread);
    publication_support::publish_candidate(&f.storage, &f.store, &prior, &source_head, 117);
    f.source = publication_support::head(&f.storage, &f.store, &source_head);
    let mapping = materialize(&f, f.source.newest_root(), 118);
    let selected = current(&f.storage, &f.store, f.thread);
    let old = f.acceptance.clone();
    f.acceptance = syndic_storage::FirstAcceptance::new(
        old.thread_id(),
        selected.thread().revision(),
        old.expected_image_label_authority(),
        old.draft_id(),
        selected.draft().revision(),
        DraftEditorCandidateActivationBindingV1::from_head(&f.source),
        mapping,
        old.expected_gate_revision(),
        old.expected_gate_state().clone(),
        old.next_draft_id(),
        old.idle_user_item_id(),
        None,
        old.session_disposal_operation_id(),
        old.admitted_at(),
    );
    let source = f
        .acceptance
        .composer_accepted_input_reference(f.store.home_id());
    committed(f.accept());
    let before = successor(&f, "retained nonempty draft");
    let checkpoint = f
        .storage
        .capture_composer_browse_checkpoint(
            &f.store,
            DraftEditorCandidateActivationBindingV1::from_head(&before),
            point(2),
            point(7),
        )
        .unwrap();
    let recalled = complete(&f, &prepare(&f, &before, source, 73));
    let candidate = adopt(&f, &recalled);
    assert_eq!(
        text(&f.storage, &f.store, candidate.root()),
        format!("{added}submitted checkpoint")
    );
    let original_transition = match recalled.settlement().unwrap().closure() {
        DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) => {
            c.adoption().transition().reference()
        }
        _ => panic!("wrong recall closure"),
    };
    let mut head = publication_support::head(&f.storage, &f.store, &before);
    let mut evicted = false;
    for id in 120..220 {
        let edit = edit_support::commit_edit(&f.storage, &f.store, &head, id, "x");
        head = edit.adopted_session().clone();
        if edit
            .adopted_history()
            .oldest_eligible()
            .is_some_and(|floor| {
                floor.cumulative_encoded_bytes() > original_transition.cumulative_encoded_bytes()
            })
        {
            evicted = true;
            break;
        }
    }
    assert!(evicted, "fixture did not exhaust ordinary undo retention");
    let restore = f
        .storage
        .prepare_composer_checkpoint_restore(
            &f.store,
            DraftEditorCandidateActivationBindingV1::from_head(&head),
            operation(230),
            checkpoint,
            point(0),
            point(0),
        )
        .unwrap();
    let restored = adopt(&f, &restore);
    assert_eq!(restored.root(), before.newest_root());
    assert_eq!(
        text(&f.storage, &f.store, restored.root()),
        "retained nonempty draft"
    );
    assert_eq!(restore.caret(), point(2));
    assert_eq!(restore.selection(), point(7));
}

#[test]
fn accepted_source_requires_the_atomic_acceptance_purpose_and_keeps_family_inventory() {
    for queued in [false, true] {
        let f = Fixture::new("recall-source", 30, queued);
        let source = f
            .acceptance
            .composer_accepted_input_reference(f.store.home_id());
        assert!(
            f.storage
                .authenticate_composer_accepted_input(&f.store, &source)
                .is_err()
        );
        committed(f.accept());
        f.storage
            .authenticate_composer_accepted_input(&f.store, &source)
            .unwrap();
        use syndic_storage::test_faults::{
            composer_acceptance_disposal_record_bytes, composer_ordinary_disposal_record_bytes,
            composer_session_record_codec_roundtrip,
        };
        let bytes = composer_acceptance_disposal_record_bytes(&f.store, &f.storage, &source);
        assert_eq!(bytes[0], 5);
        assert_eq!(
            composer_session_record_codec_roundtrip(&bytes).unwrap(),
            bytes
        );
        for end in 0..bytes.len() {
            assert!(
                composer_session_record_codec_roundtrip(&bytes[..end]).is_none(),
                "accepted receipt truncation {end}"
            );
        }
        let mut corrupted = bytes.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(composer_session_record_codec_roundtrip(&corrupted).is_none());
        let ordinary = composer_ordinary_disposal_record_bytes(&bytes);
        assert_eq!(ordinary[0], 3);
        assert_eq!(
            composer_session_record_codec_roundtrip(&ordinary).unwrap(),
            ordinary
        );
        let mut trailing = bytes;
        trailing.push(0);
        assert!(composer_session_record_codec_roundtrip(&trailing).is_none());
        assert_eq!(source.content(), f.acceptance.materialization().content());
        assert_eq!(
            syndic_storage::test_faults::syndic_v7_family_names().len(),
            99
        );
        let f = f.reopen();
        f.storage
            .authenticate_composer_accepted_input(&f.store, &source)
            .unwrap();
    }
}

#[test]
fn active_conversation_recall_replaces_nonempty_draft_and_checkpoint_restore_is_undoable() {
    for queued in [false, true] {
        let f = Fixture::new("recall-nonempty", 40, queued);
        let source = f
            .acceptance
            .composer_accepted_input_reference(f.store.home_id());
        committed(f.accept());
        let session = successor(&f, "unsent successor");
        let published = current(&f.storage, &f.store, f.thread);
        let checkpoint = f
            .storage
            .capture_composer_browse_checkpoint(
                &f.store,
                DraftEditorCandidateActivationBindingV1::from_head(&session),
                point(4),
                point(1),
            )
            .unwrap();
        let p = prepare(&f, &session, source.clone(), 73);
        let revision = f.storage.revision(&f.store).unwrap();
        for (name, malformed) in
            syndic_storage::test_faults::composer_recall_descriptor_substitutions(&p)
        {
            let outcome = execute(
                &f.store,
                f.storage.begin_composer_recall(revision, malformed),
            );
            assert!(
                matches!(
                    outcome,
                    beryl_home_store::CommandOutcome::NotCommitted { .. }
                ),
                "{name}: {outcome:?}"
            );
            assert_eq!(f.storage.revision(&f.store).unwrap(), revision);
        }
        let final_step = complete(&f, &p);
        assert_eq!(
            text(&f.storage, &f.store, session.newest_root()),
            "unsent successor"
        );
        let recalled = adopt(&f, &final_step);
        assert_eq!(
            text(&f.storage, &f.store, recalled.root()),
            "submitted checkpoint"
        );
        assert_eq!(
            recalled.candidate_generation(),
            session.newest_candidate_generation() + 1
        );
        assert_eq!(current(&f.storage, &f.store, f.thread), published);
        assert_eq!(
            final_step.caret().utf8_offset(),
            "submitted checkpoint".len() as u64
        );
        assert_eq!(final_step.caret(), final_step.selection());
        assert_eq!(
            final_step.caret().gap(),
            DraftCompositeGapWitnessV1::Unambiguous
        );
        let restore = f
            .storage
            .prepare_composer_checkpoint_restore(
                &f.store,
                recalled,
                operation(74),
                checkpoint,
                final_step.caret(),
                final_step.selection(),
            )
            .unwrap();
        let restored = adopt(&f, &restore);
        assert_eq!(restored.root(), session.newest_root());
        assert_eq!(
            restored.candidate_generation(),
            recalled.candidate_generation() + 1
        );
        assert_eq!(restore.caret(), point(4));
        assert_eq!(restore.selection(), point(1));
        let settled = restore.settlement().unwrap();
        use syndic_storage::test_faults::{
            composer_root_replacement_settlement_bytes,
            composer_root_replacement_settlement_roundtrip,
        };
        for settlement in [final_step.settlement().unwrap(), settled.clone()] {
            let bytes = composer_root_replacement_settlement_bytes(&settlement);
            assert_eq!(
                composer_root_replacement_settlement_roundtrip(&bytes).unwrap(),
                bytes
            );
            for end in [0, 1, bytes.len() / 2, bytes.len() - 1] {
                assert!(composer_root_replacement_settlement_roundtrip(&bytes[..end]).is_none());
            }
            for (name, bytes) in
                syndic_storage::test_faults::composer_root_replacement_outer_substitutions(
                    &settlement,
                )
            {
                assert!(
                    composer_root_replacement_settlement_roundtrip(&bytes).is_none(),
                    "accepted contradictory outer {name}"
                );
            }
        }
        assert!(settled.terminal_progress_receipt().is_none());
        let DraftPieceSettlementClosureV1::OrdinaryRootReplacement(c) = settled.closure() else {
            panic!("wrong restore closure")
        };
        assert!(c.adoption().adopted_history().undo_head().is_some());
        assert!(c.adoption().adopted_history().redo_head().is_none());
        let current_head = publication_support::head(&f.storage, &f.store, &session);
        assert_eq!(
            DraftEditorCandidateActivationBindingV1::from_head(&current_head),
            restored
        );
        assert_eq!(
            text(&f.storage, &f.store, f.source.newest_root()),
            "submitted checkpoint"
        );
    }
}

#[test]
fn cancellation_releases_builder_custody_without_root_or_history_adoption() {
    let f = Fixture::new("recall-cancel", 50, true);
    let source = f
        .acceptance
        .composer_accepted_input_reference(f.store.home_id());
    committed(f.accept());
    let before = successor(&f, "unsent");
    let p = prepare(&f, &before, source, 73);
    committed(execute(
        &f.store,
        f.storage
            .begin_composer_recall(f.storage.revision(&f.store).unwrap(), p.clone()),
    ));
    let terminate = f
        .storage
        .prepare_composer_recall_termination(&f.store, p.key())
        .unwrap();
    let outcome = execute(
        &f.store,
        f.storage
            .terminate_composer_recall(f.storage.revision(&f.store).unwrap(), terminate.clone()),
    );
    assert_eq!(
        f.storage
            .reconcile_composer_recall_termination(&f.store, &terminate, outcome)
            .unwrap(),
        DraftComposerRecallCommandOutcomeV1::Terminated
    );
    let after = publication_support::head(&f.storage, &f.store, &before);
    assert_eq!(after.newest_root(), before.newest_root());
    assert_eq!(after.newest_history(), before.newest_history());
    assert!(after.active_operation().is_none());
    assert!(after.session_generation() > before.session_generation());
    for (name, bytes) in syndic_storage::test_faults::composer_root_replacement_outer_substitutions(
        terminate.settlement(),
    ) {
        assert!(
            syndic_storage::test_faults::composer_root_replacement_settlement_roundtrip(&bytes)
                .is_none(),
            "accepted contradictory cancellation outer {name}"
        );
    }
}

#[test]
fn foreign_home_and_thread_sources_refuse_without_mutating_destination() {
    let f = Fixture::new("recall-foreign-source", 60, false);
    let source = f
        .acceptance
        .composer_accepted_input_reference(f.store.home_id());
    committed(f.accept());
    let other = Fixture::new("recall-foreign-home", 60, false);
    assert!(
        other
            .storage
            .authenticate_composer_accepted_input(&other.store, &source)
            .is_err()
    );
    let another = create_thread(&f.storage, &f.store, 100, 65_536);
    let durable = current(&f.storage, &f.store, another);
    let session = open_session(&f.storage, &f.store, &durable, 102, 103);
    let revision = f.store.home_revision().unwrap();
    let result = f.storage.prepare_composer_recall(
        &f.store,
        DraftComposerRecallRequestV1::new(
            DraftEditorCandidateActivationBindingV1::from_head(&session),
            operation(104),
            source,
            point(0),
            point(0),
        ),
    );
    assert!(matches!(
        result,
        Err(DraftComposerRecallErrorV1::Ineligible)
    ));
    assert_eq!(f.store.home_revision().unwrap(), revision);
}

#[test]
fn final_and_cancellation_outcomes_survive_post_commit_failure_and_fresh_candidate_reads() {
    for terminate in [false, true] {
        for fault in [
            FaultPoint::AfterCommitBeforePersist,
            FaultPoint::AfterPersist,
        ] {
            let f = Fixture::new("recall-commit-recovery", 150, true);
            let source = f
                .acceptance
                .composer_accepted_input_reference(f.store.home_id());
            committed(f.accept());
            let before = successor(&f, "retained after uncertain mutation");
            let p = prepare(&f, &before, source.clone(), 73);
            let final_step = complete(&f, &p);
            if terminate {
                let cancellation = f
                    .storage
                    .prepare_composer_recall_termination(&f.store, p.key())
                    .unwrap();
                f.faults.fail_next(fault);
                let outcome = execute(
                    &f.store,
                    f.storage.terminate_composer_recall(
                        f.storage.revision(&f.store).unwrap(),
                        cancellation.clone(),
                    ),
                );
                assert!(
                    matches!(
                        &outcome,
                        beryl_home_store::CommandOutcome::Indeterminate { .. }
                            | beryl_home_store::CommandOutcome::Committed {
                                later_failure: Some(_),
                                ..
                            }
                    ),
                    "terminate={terminate}, fault={fault:?}, outcome={outcome:?}"
                );
                eprintln!("terminate={terminate}, fault={fault:?}, original={outcome:?}");
                if matches!(
                    &outcome,
                    beryl_home_store::CommandOutcome::Indeterminate { .. }
                ) {
                    f.faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
                }
                assert!(
                    f.storage
                        .reconcile_composer_recall_termination(&f.store, &cancellation, outcome,)
                        .is_err()
                );
                if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
                    f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(f.store.home_revision().is_err());
                }
                let mut recovery = f.store.recover_same_home().unwrap();
                let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
                {
                    let access = recovery.recovery_access().unwrap();
                    let revision = fresh.revision_candidate(&access).unwrap();
                    for _ in 0..2 {
                        assert_eq!(
                            fresh
                                .resume_composer_recall_termination_candidate(
                                    &access,
                                    &cancellation
                                )
                                .unwrap(),
                            DraftComposerRecallCommandOutcomeV1::Terminated
                        );
                    }
                    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
                    assert_eq!(
                        cancellation.candidate().unwrap().root(),
                        before.newest_root()
                    );
                }
                let store = recovery.publish().unwrap();
                fresh
                    .authenticate_composer_accepted_input(&store, &source)
                    .unwrap();
            } else {
                f.faults.fail_next(fault);
                let outcome = execute(
                    &f.store,
                    f.storage.commit_composer_root_replacement(
                        f.storage.revision(&f.store).unwrap(),
                        final_step.clone(),
                    ),
                );
                assert!(
                    matches!(
                        &outcome,
                        beryl_home_store::CommandOutcome::Indeterminate { .. }
                            | beryl_home_store::CommandOutcome::Committed {
                                later_failure: Some(_),
                                ..
                            }
                    ),
                    "terminate={terminate}, fault={fault:?}, outcome={outcome:?}"
                );
                eprintln!("terminate={terminate}, fault={fault:?}, original={outcome:?}");
                if matches!(
                    &outcome,
                    beryl_home_store::CommandOutcome::Indeterminate { .. }
                ) {
                    f.faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
                }
                assert!(
                    f.storage
                        .reconcile_composer_root_replacement(&f.store, &final_step, outcome)
                        .is_err()
                );
                if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
                    f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
                    assert!(f.store.home_revision().is_err());
                }
                let mut recovery = f.store.recover_same_home().unwrap();
                let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
                {
                    let access = recovery.recovery_access().unwrap();
                    let revision = fresh.revision_candidate(&access).unwrap();
                    let first = fresh
                        .resume_composer_root_replacement_candidate(&access, &final_step)
                        .unwrap();
                    assert!(matches!(
                        first,
                        DraftComposerRecallCommandOutcomeV1::Adopted(_)
                    ));
                    assert_eq!(
                        fresh
                            .resume_composer_root_replacement_candidate(&access, &final_step)
                            .unwrap(),
                        first
                    );
                    assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
                    assert_eq!(
                        final_step.candidate().unwrap().candidate_generation(),
                        before.newest_candidate_generation() + 1
                    );
                }
                let store = recovery.publish().unwrap();
                fresh
                    .authenticate_composer_accepted_input(&store, &source)
                    .unwrap();
                assert_eq!(
                    text(&fresh, &store, final_step.candidate().unwrap().root()),
                    "submitted checkpoint"
                );
            }
        }
    }
}

#[test]
fn definite_final_noncommit_preserves_draft_and_can_cancel_original_builder() {
    let f = Fixture::new("recall-definite-noncommit", 180, true);
    let source = f
        .acceptance
        .composer_accepted_input_reference(f.store.home_id());
    committed(f.accept());
    let before = successor(&f, "not lost");
    let p = prepare(&f, &before, source, 73);
    let final_step = complete(&f, &p);
    f.faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = execute(
        &f.store,
        f.storage.commit_composer_root_replacement(
            f.storage.revision(&f.store).unwrap(),
            final_step.clone(),
        ),
    );
    assert!(matches!(
        outcome,
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert!(
        f.storage
            .reconcile_composer_root_replacement(&f.store, &final_step, outcome)
            .is_err()
    );
    assert!(final_step.candidate().is_none());
    let mut recovery = f.store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    {
        let access = recovery.recovery_access().unwrap();
        assert_eq!(
            fresh
                .resume_composer_root_replacement_candidate(&access, &final_step)
                .unwrap(),
            DraftComposerRecallCommandOutcomeV1::NotCommitted
        );
        let cancellation = fresh
            .prepare_composer_recall_termination_candidate(&access, p.key())
            .unwrap();
        let mut command = beryl_home_store::HomeCommand::new(access.home_revision().unwrap());
        command
            .add(
                fresh
                    .terminate_composer_recall_candidate(
                        &access,
                        fresh.revision_candidate(&access).unwrap(),
                        cancellation.clone(),
                    )
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            fresh
                .reconcile_composer_recall_termination_candidate(
                    &access,
                    &cancellation,
                    access.execute(command)
                )
                .unwrap(),
            DraftComposerRecallCommandOutcomeV1::Terminated
        );
        assert_eq!(
            cancellation.candidate().unwrap().root(),
            before.newest_root()
        );
    }
    let store = recovery.publish().unwrap();
    assert_eq!(text(&fresh, &store, before.newest_root()), "not lost");
}

#[test]
fn acceptance_purpose_reconciliation_rejects_substituted_source_revisions() {
    for queued in [false, true] {
        let f = Fixture::new("recall-exact-acceptance", 200, queued);
        committed(f.accept());
        assert_eq!(
            f.storage
                .first_acceptance_status(&f.store, &f.acceptance, fixture::read_limit())
                .unwrap(),
            f.expected_status()
        );
        for field in 0..3 {
            let a = &f.acceptance;
            let substituted = syndic_storage::FirstAcceptance::new(
                a.thread_id(),
                if field == 0 {
                    a.expected_thread_revision().checked_next().unwrap()
                } else {
                    a.expected_thread_revision()
                },
                a.expected_image_label_authority(),
                a.draft_id(),
                if field == 1 {
                    a.expected_draft_revision().checked_next().unwrap()
                } else {
                    a.expected_draft_revision()
                },
                a.candidate(),
                a.materialization(),
                if field == 2 {
                    a.expected_gate_revision().checked_next().unwrap()
                } else {
                    a.expected_gate_revision()
                },
                a.expected_gate_state().clone(),
                a.next_draft_id(),
                a.idle_user_item_id(),
                a.asset_reference_set(),
                a.session_disposal_operation_id(),
                a.admitted_at(),
            );
            assert_eq!(
                f.storage
                    .first_acceptance_status(&f.store, &substituted, fixture::read_limit())
                    .unwrap(),
                syndic_storage::FirstAcceptanceStatus::Collision
            );
        }
    }
}
