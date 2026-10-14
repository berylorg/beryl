use super::*;

#[test]
fn select_all_mixed_replacement_accepts_empty_source_lane_and_authenticated_proposal_removals() {
    let fixture = evidence_fixture::fixture("local-mixed-source-empty", 171);
    let store = &fixture.store;
    let storage = fixture.storage.clone();
    let (mut host, base) = activated(storage.clone(), store, fixture.thread, 172, 173);
    let original = "before  after\n\n```rust\nlet sample = 1;\n```\n\n| A | B |\n| --- | --- |\n| one | two |\n";
    let bytes = original.len() as u64;
    let lines = original.bytes().filter(|byte| *byte == b'\n').count() as u64 + 1;
    let text = commit_text(&mut host, store, base, 214, 0, 0, original, bytes, lines);
    let first = InlineObjectNeighbor::new(
        InlineObjectId::new(u128::from_be_bytes([0xb1; 16])),
        InlineObjectOrder::new(1),
    );
    let second = InlineObjectNeighbor::new(
        InlineObjectId::new(u128::from_be_bytes([0xb2; 16])),
        InlineObjectOrder::new(2),
    );
    let before = SourcePosition::new(ByteOffset::new(7), InlineObjectGap::before(first));
    let between = SourcePosition::new(
        ByteOffset::new(7),
        InlineObjectGap::between(first, second).unwrap(),
    );
    let after = SourcePosition::new(ByteOffset::new(7), InlineObjectGap::after(second));
    let marked = commit_items(
        &mut host,
        store,
        text,
        2141,
        range(source_position(7), source_position(7)),
        vec![
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(first.id(), ByteOffset::new(7), first.order(), 17, 5),
            }),
            MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    second.id(),
                    ByteOffset::new(7),
                    second.order(),
                    17,
                    5,
                ),
            }),
        ],
        MutationPositions::collapsed(after),
        vec![
            labeled_marker_metadata(
                first.id(),
                ImageLabelOrdinal::new(1).unwrap(),
                asset_id_for_object(first.id()),
            ),
            labeled_marker_metadata(
                second.id(),
                ImageLabelOrdinal::new(2).unwrap(),
                asset_id_for_object(second.id()),
            ),
        ],
        bytes,
        lines,
    );
    let key = mutation_key(marked, 2216);
    predecessor_reads::assert_predecessor_markers(
        &mut host,
        &storage,
        store,
        marked,
        70_000,
        7,
        [first, second],
    );
    let start = source_position(0);
    let end = source_position(bytes);
    let replacement = range(start, end);
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::new(end, start, end),
            replacement,
            lines - 1,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    )
    .with_replayable_producer(MutationProducerIdentity::new(12216));
    let mut editor = RangeEditCoordinator::new(
        marked.range_binding(),
        MutationLimits::new(16, 65_536)
            .unwrap()
            .with_object_limits(16, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();
    let evidence = editor.request_evidence(key).unwrap();
    assert!(matches!(drive(&mut host, store, marked, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Begin { begin, pass: evidence }), ComposerHostMutationEvidenceOutcome::Started(actual) if actual == evidence));
    let removals = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![
            MutationPageItem::Object(ObjectChange::Remove {
                target: ObjectTarget::new(range(before, between), first.id(), first.order())
                    .unwrap(),
            }),
            MutationPageItem::Object(ObjectChange::Remove {
                target: ObjectTarget::new(range(between, after), second.id(), second.order())
                    .unwrap(),
            }),
        ],
    )
    .unwrap();
    let tail = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(1),
            1,
            removals.cumulative_identity(),
        ),
        MutationCursor::new(2),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "".into(),
        }],
    )
    .unwrap();
    for page in [&removals, &tail] {
        let acknowledgement = editor.submit_evidence_page(evidence, page.clone()).unwrap();
        assert!(
            matches!(drive(&mut host, store, marked, &fixture.assets, ComposerHostMutationEvidenceRequest::Page {
            pass: evidence, page: page.clone(), metadata: Box::new([]),
        }), ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == evidence)
        );
        editor.acknowledge_evidence_page(acknowledgement).unwrap();
    }
    let finish = MutationFinishInput::with_positions_intent(
        key,
        editor.stream_finish(key, MutationLane::Source).unwrap(),
        editor.stream_finish(key, MutationLane::Proposal).unwrap(),
        LogicalExtent::new(0, 0),
        MutationPositionsIntent::ReplacementEnd {
            replacement,
            inserted_utf8_bytes: 0,
        },
    );
    assert_eq!(finish.source(), empty_finish());
    assert_eq!(finish.proposal().totals.pages, 2);
    assert_eq!(finish.proposal().totals.objects, 2);
    editor.finish_evidence(evidence, finish).unwrap();
    assert!(matches!(drive(&mut host, store, marked, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }), ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key));
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    assert!(matches!(
        predecessor_reads::read_markers(
            &mut host,
            store,
            marked,
            70_001,
            7,
            beryl_app::composer_host::ComposerHostRequestPurpose::Selection,
        ),
        Err(ComposerHostError::Range(
            syndic_storage::DraftPieceRangeSourceErrorV1::StaleSession
        ))
    ));
    predecessor_reads::assert_predecessor_markers(
        &mut host,
        &storage,
        store,
        marked,
        70_002,
        7,
        [first, second],
    );
    for (index, page) in [removals, tail].into_iter().enumerate() {
        host.stage_mutation_page(
            store,
            MutationPageRequest::new(page).with_pass(staging),
            Box::new([]),
        )
        .unwrap();
        predecessor_reads::assert_predecessor_markers(
            &mut host,
            &storage,
            store,
            marked,
            70_003 + index as u64,
            7,
            [first, second],
        );
    }
    host.finish_mutation_input(store, finish).unwrap();
    let (committed, positions) = commit_mapped(&mut host, store, key);
    assert_eq!(candidate_text(storage.clone(), store, committed), b"");
    assert_eq!(committed.root().summary().marker_count(), 0);
    assert_ne!(committed.history(), marked.history());
    assert_eq!(positions, MutationPositions::collapsed(source_position(0)));
    assert_eq!(host.mutation_status(), None);
    for object in [first, second] {
        assert!(
            storage
                .draft_marker_identity(
                    store,
                    committed.root(),
                    SyndicDraftMarkerId::from_bytes(object.id().get().to_be_bytes())
                )
                .unwrap()
                .is_none()
        );
    }
}
