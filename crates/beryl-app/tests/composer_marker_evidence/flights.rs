use super::*;
use beryl_app::composer_host::ComposerHostServiceDisposalCompletion;
use beryl_home_store::test_faults::FaultPoint;
use syndic_storage::{
    DraftPieceReconciledCommandV1, DraftPieceTransactionOutcomeV1,
    StagedDraftPieceDurableClassificationV1 as Durable, StagedDraftPieceOutcomeStateV1 as State,
};

fn stage_insert(
    fixture: &support::Fixture,
    host: &mut SyndicComposerHost,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
) -> (MutationKey, InlineObjectId) {
    let asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"flight-image");
    let object = InlineObjectId::new(1001);
    let order = InlineObjectOrder::new(1);
    let metadata = Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]);
    let (mut editor, staging, proposal, finish) = replay::prepare(
        fixture,
        host,
        binding,
        operation,
        SourceRange::new(origin(), origin()).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(object, ByteOffset::new(0), order, 1, 1),
        })],
        metadata.clone(),
        after(object, order),
    );
    stage(
        host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        metadata,
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    (finish.key(), object)
}

fn ambiguous_transfer(
    fixture: &support::Fixture,
    host: &mut SyndicComposerHost,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
) -> MutationKey {
    let (key, _) = stage_insert(fixture, host, binding, operation);
    host.test_set_mutation_transition_limit(1);
    let faults = fixture.faults.clone();
    host.test_arm_mutation_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert!(matches!(
        host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::MutationWorkPending)
    ));
    let diagnostics = host.mutation_build_diagnostics().unwrap();
    assert_eq!(diagnostics.state, Some(State::Reconciling));
    assert_eq!(diagnostics.classification, Some(Durable::Unresolved));
    assert!(diagnostics.original_failure.is_some());
    assert_eq!(host.settlement_custody_in_use(), 1);
    assert_eq!(host.binding(), Some(binding));
    key
}

#[test]
fn cancellation_resumes_the_submitted_build_flight_before_exact_marker_noncommit() {
    let fixture = fixture("composer-marker-flight-cancel", 100);
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        101,
        102,
    );
    let key = ambiguous_transfer(&fixture, &mut host, binding, 103);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut completed = false;
    for _ in 0..128 {
        match host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        ) {
            Err(ComposerHostError::MutationWorkPending) => {
                assert_eq!(host.settlement_custody_in_use(), 1)
            }
            Ok(ComposerHostMutationOutcome::Cancelled) => {
                completed = true;
                break;
            }
            other => panic!("cancelled marker flight lost its exact outcome: {other:?}"),
        }
    }
    assert!(completed);
    let settled_binding = host.binding().unwrap();
    assert_eq!(settled_binding.root(), binding.root());
    assert_eq!(settled_binding.history(), binding.history());
    assert_eq!(settled_binding.range_binding(), binding.range_binding());
    assert_eq!(
        settled_binding.candidate().candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(
        settled_binding.candidate().session_id(),
        binding.candidate().session_id()
    );
    assert!(
        settled_binding.candidate().session_generation() > binding.candidate().session_generation()
    );
    assert_eq!(host.settlement_custody_in_use(), 0);
    let diagnostics = host.mutation_build_diagnostics().unwrap();
    assert_eq!(diagnostics.classification, Some(Durable::Committed));
    assert!(diagnostics.original_failure.is_some());
    assert!(matches!(
        diagnostics.result,
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        ))
    ));
    assert!(matches!(
        host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation
        ),
        Err(ComposerHostError::MutationNotPending)
    ));
    let next_binding = host.binding().unwrap();
    let (mut editor, begin, next_key) = begin_edit(
        next_binding,
        104,
        SourceRange::new(origin(), origin()).unwrap(),
        105,
    );
    let evidence = editor.request_evidence(next_key).unwrap();
    let outcome = drive_evidence(
        &mut host,
        &fixture.store,
        next_binding,
        &fixture.assets,
        ComposerHostMutationEvidenceRequest::Begin {
            begin,
            pass: evidence,
        },
    );
    match outcome {
        ComposerHostMutationEvidenceOutcome::Started(pass) => assert_eq!(pass, evidence),
        ComposerHostMutationEvidenceOutcome::Refused { failure, .. } => {
            panic!("ordinary edit could not begin after exact cancellation drain: {failure:?}")
        }
        _ => panic!("ordinary edit did not start after exact cancellation drain"),
    }
    assert!(matches!(
        cancel_evidence(&mut host, &fixture.store, next_binding, &fixture.assets, next_key),
        ComposerHostMutationEvidenceOutcome::Refused { failure, .. }
            if matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Cancelled)
    ));
    let ordinary_binding = host.binding().unwrap();
    let (mut editor, begin, ordinary_key) = begin_edit(
        ordinary_binding,
        106,
        SourceRange::new(origin(), origin()).unwrap(),
        107,
    );
    host.begin_mutation(&fixture.store, ordinary_binding, begin)
        .unwrap();
    editor.accept_preflight(ordinary_key, None).unwrap();
    let proposal = page(
        &editor,
        ordinary_key,
        MutationLane::Proposal,
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "next".into(),
        }],
    );
    editor.accept_page(proposal.clone()).unwrap();
    host.stage_mutation_page(
        &fixture.store,
        MutationPageRequest::new(proposal),
        Box::new([]),
    )
    .unwrap();
    let finish = MutationFinishInput::new(
        ordinary_key,
        editor
            .stream_finish(ordinary_key, MutationLane::Source)
            .unwrap(),
        editor
            .stream_finish(ordinary_key, MutationLane::Proposal)
            .unwrap(),
        LogicalExtent::new(4, 1),
        MutationPositions::collapsed(SourcePosition::new(
            ByteOffset::new(4),
            InlineObjectGap::NoObjects,
        )),
    );
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    let committed = settle(&mut host, &fixture.store, ordinary_key);
    let text = fixture
        .storage
        .candidate_draft_piece_text_demand(
            &fixture.store,
            committed.candidate(),
            syndic_storage::DraftPieceTextDemandV1::Forward(0),
            256,
        )
        .unwrap();
    assert_eq!(text.value().bytes(), b"next");
}

#[test]
fn service_disposal_keeps_the_submitted_marker_flight_until_its_exact_drain() {
    let fixture = fixture("composer-marker-flight-disposal", 110);
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        111,
        112,
    );
    let key = ambiguous_transfer(&fixture, &mut host, binding, 113);
    assert_eq!(
        host.dispose_composer_service(&fixture.store).unwrap(),
        ComposerHostServiceDisposalCompletion::Pending
    );
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.settlement_custody_in_use(), 1);
    let mut completed = false;
    for _ in 0..128 {
        match host.dispose_composer_service(&fixture.store).unwrap() {
            ComposerHostServiceDisposalCompletion::Pending => {
                assert_eq!(host.settlement_custody_in_use(), 1)
            }
            ComposerHostServiceDisposalCompletion::Disposed => {
                completed = true;
                break;
            }
        }
    }
    assert!(completed);
    assert_eq!(host.settlement_custody_in_use(), 0);
    assert!(host.binding().is_none());
    let diagnostics = host.mutation_build_diagnostics().unwrap();
    assert!(diagnostics.original_failure.is_some());
    assert!(matches!(
        diagnostics.result,
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        ))
    ));
    assert!(matches!(
        host.execute_mutation(
            &fixture.store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::MutationNotPending)
    ));
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("flight disposal changed the prior candidate");
    };
    let current = syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session);
    let prior = binding.candidate();
    assert!(session.active_operation().is_none());
    assert!(current.session_generation() > prior.session_generation());
    assert_eq!(current.draft_id(), prior.draft_id());
    assert_eq!(current.session_id(), prior.session_id());
    assert_eq!(current.candidate_generation(), prior.candidate_generation());
    assert_eq!(current.root(), prior.root());
    assert_eq!(current.history(), prior.history());
    assert_eq!(current.logical_extent(), prior.logical_extent());
}

#[test]
fn committed_marker_cleanup_failure_preserves_success_and_its_distinct_diagnostic() {
    let fixture = fixture("composer-marker-committed-cleanup", 120);
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        121,
        122,
    );
    let (key, object) = stage_insert(&fixture, &mut host, binding, 123);
    host.test_set_mutation_transition_limit(1);
    let mut cleanup_pending = false;
    for _ in 0..128 {
        assert!(matches!(
            host.execute_mutation(
                &fixture.store,
                MutationCommitRequest::new(key, MutationIdentity::ROOT),
                &CommandCancellation::new()
            ),
            Err(ComposerHostError::MutationWorkPending)
        ));
        if host.mutation_build_diagnostics().unwrap().state == Some(State::CleanupPending) {
            cleanup_pending = true;
            break;
        }
    }
    assert!(cleanup_pending);
    let diagnostics = host.mutation_build_diagnostics().unwrap();
    assert_eq!(diagnostics.classification, Some(Durable::Committed));
    assert!(matches!(
        diagnostics.result,
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        ))
    ));
    assert_eq!(host.settlement_custody_in_use(), 1);
    let Some(DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(
        syndic_storage::DraftPieceSettlementProofV1::Settlement(settlement),
    ))) = diagnostics.result
    else {
        panic!("cleanup lacked the exact committed settlement");
    };
    let syndic_storage::DraftPieceSettlementOutcomeV1::Committed { successor, .. } =
        settlement.outcome()
    else {
        panic!("cleanup lacked the committed successor");
    };
    let committed_root = *successor;
    assert_eq!(
        fixture
            .storage
            .draft_marker_identity(
                &fixture.store,
                committed_root,
                SyndicDraftMarkerId::from_bytes(object.get().to_be_bytes()),
            )
            .unwrap()
            .unwrap()
            .marker_id(),
        SyndicDraftMarkerId::from_bytes(object.get().to_be_bytes())
    );
    let faults = fixture.faults.clone();
    host.test_arm_mutation_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterPersist);
    });
    let committed = settle(&mut host, &fixture.store, key);
    assert_eq!(committed.root().summary().marker_count(), 1);
    assert_eq!(committed.root(), committed_root);
    assert_eq!(host.binding(), Some(committed));
    assert_eq!(host.settlement_custody_in_use(), 0);
    let diagnostics = host.mutation_build_diagnostics().unwrap();
    assert_eq!(diagnostics.classification, Some(Durable::Committed));
    assert!(diagnostics.original_failure.is_none());
    assert!(diagnostics.cleanup_failure.is_some());
    assert!(matches!(
        diagnostics.result,
        Some(DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        ))
    ));
}

fn unavailable_build_after_recovery(committed: bool) {
    let fixture = fixture(
        "composer-build-unavailable",
        if committed { 130 } else { 140 },
    );
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        131,
        132,
    );
    let (key, _) = stage_insert(&fixture, &mut host, binding, 133);
    host.test_set_mutation_transition_limit(1);
    let mut armed = false;
    let mut failed = false;
    for _ in 0..256 {
        let diagnostics = host.mutation_build_diagnostics();
        if armed
            && diagnostics.as_ref().is_some_and(|diagnostics| {
                if committed {
                    diagnostics.state == Some(State::CleanupPending)
                        && diagnostics.cleanup_failure.is_some()
                } else {
                    diagnostics.state == Some(State::Finalizing)
                        && diagnostics.local_failure.is_some()
                }
            })
        {
            failed = true;
            break;
        }
        if !armed
            && diagnostics.as_ref().is_some_and(|diagnostics| {
                if committed {
                    diagnostics.state == Some(State::CleanupPending)
                } else {
                    diagnostics.state.is_none()
                        && matches!(
                            diagnostics.result,
                            Some(DraftPieceReconciledCommandV1::Pending(_))
                        )
                }
            })
        {
            let faults = fixture.faults.clone();
            host.test_arm_mutation_before_execute_fault(move |_, _| {
                faults.fail_next(if committed {
                    FaultPoint::BeforeCommit
                } else {
                    FaultPoint::AfterPersist
                });
            });
            armed = true;
        }
        assert!(matches!(
            host.execute_mutation(
                &fixture.store,
                MutationCommitRequest::new(key, MutationIdentity::ROOT),
                &CommandCancellation::new()
            ),
            Err(ComposerHostError::MutationWorkPending)
        ));
    }
    assert!(failed);
    let historical = host
        .mutation_build_diagnostics()
        .unwrap()
        .result
        .unwrap()
        .clone();
    assert_eq!(
        matches!(
            &historical,
            DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(_))
        ),
        committed
    );
    let recovery = fixture.store.recover_same_home().unwrap();
    let _storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    for _ in 0..2 {
        let error = host
            .execute_mutation(
                &store,
                MutationCommitRequest::new(key, MutationIdentity::ROOT),
                &CommandCancellation::new(),
            )
            .unwrap_err();
        assert!(
            if committed {
                matches!(error, ComposerHostError::MutationCommittedUnavailable)
            } else {
                matches!(error, ComposerHostError::MutationAdmittedWorkUnavailable)
            },
            "unexpected unavailable classification: {error}"
        );
        assert_eq!(host.binding(), Some(binding));
        assert_eq!(host.settlement_custody_in_use(), 1);
        let diagnostics = host.mutation_build_diagnostics().unwrap();
        assert_eq!(diagnostics.state, Some(State::Unavailable));
        assert_eq!(diagnostics.result, Some(&historical));
        assert_eq!(diagnostics.cleanup_failure.is_some(), committed);
        if !committed {
            assert!(diagnostics.later_failure.is_some());
        }
    }
}

#[test]
fn committed_cleanup_unavailable_preserves_authenticated_commit_classification() {
    unavailable_build_after_recovery(true);
}

#[test]
fn intermediate_build_unavailable_does_not_claim_the_edit_committed() {
    unavailable_build_after_recovery(false);
}
