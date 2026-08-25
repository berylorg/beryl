use super::*;

#[cfg(feature = "test-faults")]
#[test]
fn disposed_absent_corrupt_and_selector_drift_are_typed_and_atomic() {
    let (_home, store, storage, thread) = fixture("typed-failures", 80);
    populate(storage, &store, thread, 81);
    let mut host = SyndicComposerHost::new(storage);
    let request = activation(thread, 82, 83, Vec::new());
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(&store, request.clone(), &CommandCancellation::new())
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        ),
    ));
    host.dispose_composer_service(&store).unwrap();
    let mut replacement = SyndicComposerHost::new(storage);
    assert!(matches!(
        replacement.activate(&store, request, &CommandCancellation::new()),
        Ok(ComposerHostActivationOutcome::StaleDisposed(_))
    ));
    assert_eq!(host.binding(), None);
    assert_eq!(replacement.binding(), None);

    let (_home, store, storage, thread) = fixture("missing-root", 84);
    populate(storage, &store, thread, 85);
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(
            &store,
            activation(thread, 85, 95, Vec::new()),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture activation failed");
    };
    committed(execute(
        &store,
        delete_draft_piece_immutable_record(
            &store,
            storage,
            binding.root(),
            DraftPieceImmutableDeletion::Root,
        ),
    ));
    assert!(matches!(
        run(
            &mut host,
            &store,
            binding,
            1,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Historical(binding.root()),
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 65_536,
            },
        ),
        Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::Absent
        ))
    ));

    for (case, corruption) in [
        DraftPieceDescendantCorruption::NewlineAggregate,
        DraftPieceDescendantCorruption::LogicalLineAggregate,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, thread) = fixture("corrupt-summary", 88 + case as u8);
        let before = current(storage, &store, thread);
        let pieces = (0..130)
            .map(|_| DraftPieceV1::Text("x\n".to_owned()))
            .collect();
        let build = transaction(
            storage,
            &store,
            &before,
            91 + case as u8,
            93 + case as u8,
            vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
            point(260),
        );
        run_transaction(storage, &store, &build, 2);
        let mut host = SyndicComposerHost::new(storage);
        let ComposerHostActivationOutcome::Activated { binding, .. } = host
            .activate(
                &store,
                activation(thread, 91 + case as u8, 93 + case as u8, Vec::new()),
                &CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("fixture activation failed");
        };
        committed(execute(
            &store,
            inject_draft_piece_descendant_corruption(
                &store,
                storage,
                binding.root(),
                DraftPieceDescendantTarget::Sequence,
                corruption,
            ),
        ));
        assert!(matches!(
            run(
                &mut host,
                &store,
                binding,
                1,
                ComposerHostRequestKind::Text {
                    target: ComposerHostReadTarget::Candidate,
                    demand: DraftPieceTextDemandV1::Forward(0),
                    max_bytes: 65_536,
                },
            ),
            Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Invariant
            ))
        ));
    }

    let (_home, store, storage, thread) = fixture("selector-conflict", 101);
    let before = current(storage, &store, thread);
    let next_thread_revision = before.thread().revision().checked_next().unwrap();
    let advanced_thread = ThreadRecord::new(
        before.thread().id(),
        SelectedPathProof::new(
            before.thread().committed_tail(),
            next_thread_revision,
            before.thread().selected_path_digest(),
        ),
        before.thread().current_draft_id(),
        before.thread().lineage(),
        before.thread().image_label_frontiers(),
        before.thread().context_owner_id(),
    );
    let advanced_index = DraftByThreadRecord::new(
        before.thread().id(),
        before.draft().id(),
        before.draft().revision(),
        next_thread_revision,
    );
    let mut host = SyndicComposerHost::new(storage);
    host.test_arm_activation_after_selector_fault(move |store, storage| {
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Thread(advanced_thread)).unwrap();
        batch
            .put(FixtureRecord::DraftByThread(advanced_index))
            .unwrap();
        committed(execute(
            store,
            storage.fixture_contribution(storage.revision(store).unwrap(), batch),
        ));
    });
    assert!(matches!(
        host.activate(
            &store,
            activation(thread, 104, 105, Vec::new()),
            &CommandCancellation::new(),
        ),
        Ok(ComposerHostActivationOutcome::SelectorConflict(_))
    ));
    assert_eq!(host.binding(), None);
}
