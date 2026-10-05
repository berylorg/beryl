use super::*;
use syndic_storage::DraftPrivateClipboardSourceV1;

fn assert_private_begin_replay(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: syndic_storage::PreparedDraftMutationStagingCommandV1,
) {
    let revision = storage.revision(store).unwrap();
    let outcome = execute(
        store,
        storage.draft_mutation_staging_command(revision, prepared.clone()),
    );
    assert!(
        matches!(&outcome, CommandOutcome::NotCommitted {
        evidence: CommandError::EmptyContribution { domain }
    } if *domain == "syndic"),
        "expected immutable private begin replay: {outcome:?}"
    );
    assert_eq!(
        storage
            .reconcile_draft_mutation_staging_command_outcome(store, &prepared, outcome)
            .unwrap(),
        syndic_storage::DraftMutationStagingReconcileV1::TargetSelected
    );
    assert_eq!(storage.revision(store).unwrap(), revision);
}

fn foreign_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    seed: u8,
) -> DraftEditorCandidateSessionV1 {
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    committed(execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-private",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    open_session(
        storage,
        store,
        &current(storage, store, thread),
        seed.wrapping_add(2),
        seed.wrapping_add(3),
    )
}

fn cut_marker(
    storage: &SyndicStorage,
    store: &HomeStore,
    source: &DraftEditorCandidateSessionV1,
    marker: DraftPieceMarkerV1,
    operation: u8,
) -> (DraftEditorCandidateSessionV1, DraftPrivateClipboardSourceV1) {
    let occurrence = storage
        .draft_marker_identity(store, source.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();
    let position = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let successor = readiness_support::complete_marker_edit(
        storage,
        store,
        source,
        operation,
        DraftPieceReplacementV1::new(position, position, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(marker),
            },
        ),
    );
    let descriptor = DraftPrivateClipboardSourceV1::from_committed_cut(
        DraftPieceSettlementKeyV1::new(
            source.draft_id(),
            source.session_id(),
            DraftPieceOperationIdV1::from_bytes([operation; 16]),
        ),
        successor.newest_candidate_generation(),
        successor.newest_root(),
        source.newest_root(),
    )
    .unwrap();
    (successor, descriptor)
}

#[test]
fn foreign_candidate_and_cut_allocate_while_local_cut_preserves() {
    let (_home, store, storage, thread) = fixture("private-assignment", 180);
    let (source, marker) = readiness_support::marked_session(&storage, &store, thread, 181);
    let destination = foreign_session(&storage, &store, 195);
    let candidate = DraftPrivateClipboardSourceV1::candidate(&source).unwrap();
    {
        assert!(
            storage
                .prepare_draft_marker_label_readiness_page(
                    &store,
                    DraftMarkerLabelReadinessPageRequestV1::new(
                        owner(&destination, 204),
                        DraftMarkerAdmissionCommandIdV1::from_bytes([204; 16]),
                        NonZeroU64::MIN,
                        true,
                        DraftMarkerLabelReadinessDispositionV1::Reuse,
                        Box::new([DraftMarkerReadinessSourceAssociationV1::new(
                            SyndicDraftMarkerId::from_bytes([204; 16]),
                            candidate.marker_selector(marker.marker_id())
                        )]),
                        None
                    )
                )
                .is_err()
        );
        let proof = eof_then_assign(
            &storage,
            &store,
            owner(&destination, 205),
            206,
            DraftMarkerLabelReadinessDispositionV1::Allocate,
            vec![
                DraftMarkerReadinessSourceAssociationV1::new(
                    SyndicDraftMarkerId::from_bytes([207; 16]),
                    candidate.marker_selector(marker.marker_id()),
                ),
                DraftMarkerReadinessSourceAssociationV1::new(
                    SyndicDraftMarkerId::from_bytes([208; 16]),
                    candidate.marker_selector(marker.marker_id()),
                ),
            ],
        );
        assert!(proof.allocation_range().is_some());
        let assigned = storage
            .inspect_draft_marker_label_readiness_proof_for_test(&store, &proof)
            .unwrap();
        assert_eq!(
            assigned_label(&assigned, 207),
            assigned_label(&assigned, 208)
        );
        assert_eq!(
            assigned_label(&assigned, 207),
            proof.allocation_range().unwrap().first()
        );
    }
    let (successor, cut) = cut_marker(&storage, &store, &source, marker, 210);
    assert!(
        storage
            .draft_private_clipboard_source_is_current(&store, cut)
            .unwrap()
    );
    assert!(
        !storage
            .draft_private_clipboard_source_is_current(&store, candidate)
            .unwrap()
    );
    for (operation, destination) in [(211, &destination), (212, &successor)] {
        let proof = eof_then_assign(
            &storage,
            &store,
            owner(destination, operation),
            operation.wrapping_add(5),
            DraftMarkerLabelReadinessDispositionV1::Allocate,
            vec![DraftMarkerReadinessSourceAssociationV1::new(
                SyndicDraftMarkerId::from_bytes([213; 16]),
                cut.marker_selector(marker.marker_id()),
            )],
        );
        let assigned = storage
            .inspect_draft_marker_label_readiness_proof_for_test(&store, &proof)
            .unwrap();
        if destination.thread_id() == successor.thread_id() {
            assert!(proof.allocation_range().is_none());
            assert_eq!(assigned_label(&assigned, 213), marker.label());
        } else {
            assert_eq!(
                assigned_label(&assigned, 213),
                proof.allocation_range().unwrap().first()
            );
        }
    }
}

#[test]
fn stale_origin_refuses_private_begin_and_committed_replay_ignores_origin_changes() {
    let (_home, store, storage, thread) = fixture("private-begin-fence", 220);
    let current = current(&storage, &store, thread);
    let origin = open_session(&storage, &store, &current, 221, 222);
    let source = DraftPrivateClipboardSourceV1::candidate(&origin).unwrap();
    let destination = foreign_session(&storage, &store, 223);
    let second_destination = foreign_session(&storage, &store, 235);
    let make_begin = |destination: &DraftEditorCandidateSessionV1, operation| {
        let begin = begin_input(
            DraftMutationStagingIdentityV1::new(
                destination.draft_id(),
                destination.session_id(),
                DraftMutationOperationIdV1::from_bytes([operation; 16]),
            ),
            &destination,
        );
        let fence = storage.prepare_draft_private_clipboard_source_fence(begin, source);
        storage
            .prepare_draft_mutation_staging_private_begin(begin, &destination, fence)
            .unwrap()
    };
    let admitted = make_begin(&destination, 228);
    let refused = make_begin(&second_destination, 229);
    let unfenced = storage
        .prepare_draft_mutation_staging_begin(refused.target_head().begin(), &second_destination)
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), admitted.clone()),
    ));
    complete_staged(
        &storage,
        &store,
        &origin,
        230,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("changed".to_owned())],
        ),
        DraftLogicalExtentV1::new(7, 1),
    );
    assert!(
        !storage
            .draft_private_clipboard_source_is_current(&store, source)
            .unwrap()
    );
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(revision, refused)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_private_begin_replay(&storage, &store, admitted);
    assert_eq!(storage.revision(&store).unwrap(), revision);
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(revision, unfenced),
    ));
}

#[test]
fn private_cut_fence_rejects_wrong_settlement_and_stale_successor() {
    let (_home, store, storage, thread) = fixture("private-cut-fence", 140);
    let (origin, marker) = readiness_support::marked_session(&storage, &store, thread, 141);
    let (successor, cut) = cut_marker(&storage, &store, &origin, marker, 146);
    let destination = foreign_session(&storage, &store, 150);
    let second_destination = foreign_session(&storage, &store, 160);
    let wrong = DraftPrivateClipboardSourceV1::from_committed_cut(
        DraftPieceSettlementKeyV1::new(
            origin.draft_id(),
            origin.session_id(),
            DraftPieceOperationIdV1::from_bytes([147; 16]),
        ),
        successor.newest_candidate_generation(),
        successor.newest_root(),
        origin.newest_root(),
    )
    .unwrap();
    assert!(
        !storage
            .draft_private_clipboard_source_is_current(&store, wrong)
            .unwrap()
    );
    let make_begin = |source, operation| {
        let begin = begin_input(
            DraftMutationStagingIdentityV1::new(
                destination.draft_id(),
                destination.session_id(),
                DraftMutationOperationIdV1::from_bytes([operation; 16]),
            ),
            &destination,
        );
        let fence = storage.prepare_draft_private_clipboard_source_fence(begin, source);
        storage
            .prepare_draft_mutation_staging_private_begin(begin, &destination, fence)
            .unwrap()
    };
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(revision, make_begin(wrong, 155))
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    let stale = make_begin(cut, 156);
    let begin = begin_input(
        DraftMutationStagingIdentityV1::new(
            second_destination.draft_id(),
            second_destination.session_id(),
            DraftMutationOperationIdV1::from_bytes([158; 16]),
        ),
        &second_destination,
    );
    let admitted = storage
        .prepare_draft_mutation_staging_private_begin(
            begin,
            &second_destination,
            storage.prepare_draft_private_clipboard_source_fence(begin, cut),
        )
        .unwrap();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), admitted.clone()),
    ));
    complete_staged(
        &storage,
        &store,
        &successor,
        157,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("x".to_owned())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(revision, stale)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_private_begin_replay(&storage, &store, admitted);
    assert_eq!(storage.revision(&store).unwrap(), revision);
}

#[test]
fn assigned_private_markers_still_require_current_origin_at_begin() {
    let (_home, store, storage, thread) = fixture("private-marker-fence", 100);
    let (origin, marker) = readiness_support::marked_session(&storage, &store, thread, 101);
    let destination = foreign_session(&storage, &store, 110);
    let source = DraftPrivateClipboardSourceV1::candidate(&origin).unwrap();
    let operation = owner(&destination, 115);
    let proof = eof_then_assign(
        &storage,
        &store,
        operation,
        116,
        DraftMarkerLabelReadinessDispositionV1::Allocate,
        vec![DraftMarkerReadinessSourceAssociationV1::new(
            SyndicDraftMarkerId::from_bytes([117; 16]),
            source.marker_selector(marker.marker_id()),
        )],
    );
    let begin = begin_input(
        DraftMutationStagingIdentityV1::new(
            destination.draft_id(),
            destination.session_id(),
            DraftMutationOperationIdV1::from_bytes([115; 16]),
        ),
        &destination,
    );
    let prepared = storage
        .prepare_draft_mutation_staging_private_marker_begin(begin, &destination, proof, source)
        .unwrap();
    complete_staged(
        &storage,
        &store,
        &origin,
        120,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("x".to_owned())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        execute(
            &store,
            storage.draft_mutation_staging_command(revision, prepared)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert!(
        storage
            .draft_mutation_staging_head(&store, begin.identity())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .draft_marker_admission_publication_snapshot_for_test(&store, operation, &[])
            .unwrap()
            .head()
            .unwrap()
            .lifecycle(),
        syndic_storage::DraftMarkerAdmissionLifecycleV1::Ready
    );
}

#[test]
fn private_fence_cannot_be_substituted_into_another_destination_operation() {
    let (_home, store, storage, thread) = fixture("private-fence-binding", 60);
    let current = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &current, 61, 62);
    let source = DraftPrivateClipboardSourceV1::candidate(&session).unwrap();
    let begin = |operation| {
        begin_input(
            DraftMutationStagingIdentityV1::new(
                session.draft_id(),
                session.session_id(),
                DraftMutationOperationIdV1::from_bytes([operation; 16]),
            ),
            &session,
        )
    };
    let fence = storage.prepare_draft_private_clipboard_source_fence(begin(63), source);
    assert!(matches!(
        storage.prepare_draft_mutation_staging_private_begin(begin(64), &session, fence),
        Err(syndic_storage::DraftMutationStagingErrorV1::Invalid)
    ));
}
