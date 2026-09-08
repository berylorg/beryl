use super::*;

#[test]
fn leading_marker_removal_and_remaining_text_delete_share_one_atomic_mutation() {
    let (_home, store, storage, thread) = fixture("leading-marker-delete", 61);
    let (mut host, base) = activated(storage.clone(), &store, thread, 62, 63);
    let text = commit_text(&mut host, &store, base, 64, 0, 0, "AB", 2, 1);
    let id = InlineObjectId::new(0x8001);
    let order = InlineObjectOrder::new(1);
    let before = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order)),
    );
    let after = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    );
    let inserted = commit_items(
        &mut host,
        &store,
        text,
        65,
        range(source_position(0), source_position(0)),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 17, 5),
        })],
        MutationPositions::collapsed(before),
        vec![labeled_marker_metadata(
            id,
            ImageLabelOrdinal::new(1).unwrap(),
            asset_id_for_object(id),
        )],
        2,
        1,
    );
    let target = ObjectTarget::new(range(before, after), id, order).unwrap();
    let committed = commit_items(
        &mut host,
        &store,
        inserted,
        66,
        range(before, source_position(1)),
        vec![
            MutationPageItem::Object(ObjectChange::Remove { target }),
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            },
        ],
        MutationPositions::collapsed(source_position(0)),
        Vec::new(),
        1,
        1,
    );
    assert_eq!(candidate_text(storage.clone(), &store, committed), b"B");
    assert_eq!(committed.root().summary().marker_count(), 0);
}

#[test]
fn marker_insert_replace_move_and_remove_commit_through_paged_staging() {
    let (_home, store, storage, thread) = fixture("markers", 71);
    let (mut host, base) = activated(storage.clone(), &store, thread, 72, 73);
    let text = commit_text(&mut host, &store, base, 74, 0, 0, "x", 1, 1);
    let id_value = u128::from_be_bytes([0x80, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0xfe]);
    let id = InlineObjectId::new(id_value);
    let label = ImageLabelOrdinal::new(9).unwrap();
    let asset_id = asset_id_for_object(id);
    let order_one = InlineObjectOrder::new(1);
    let one = source_position(1);
    let after_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_one)),
    );
    let inserted = commit_items(
        &mut host,
        &store,
        text,
        75,
        range(one, one),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(1), order_one, 17, 5),
        })],
        MutationPositions::collapsed(after_one),
        vec![labeled_marker_metadata(id, label, asset_id)],
        1,
        1,
    );
    let marker_id = SyndicDraftMarkerId::from_bytes(id_value.to_be_bytes());
    assert_marker(
        storage.clone(),
        &store,
        inserted,
        marker_id,
        1,
        1,
        label,
        asset_id,
    );

    let before_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order_one)),
    );
    let target_one = ObjectTarget::new(range(before_one, after_one), id, order_one).unwrap();
    let order_two = InlineObjectOrder::new(2);
    let after_two_at_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_two)),
    );
    let replaced = commit_items(
        &mut host,
        &store,
        inserted,
        76,
        target_one.range(),
        vec![MutationPageItem::Object(ObjectChange::Replace {
            target: target_one,
            object: SuccessorObject::new(id, ByteOffset::new(1), order_two, 17, 5),
        })],
        MutationPositions::collapsed(after_two_at_one),
        vec![labeled_marker_metadata(id, label, asset_id)],
        1,
        1,
    );
    assert_marker(
        storage.clone(),
        &store,
        replaced,
        marker_id,
        1,
        2,
        label,
        asset_id,
    );

    let before_two_at_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order_two)),
    );
    let target_two =
        ObjectTarget::new(range(before_two_at_one, after_two_at_one), id, order_two).unwrap();
    let after_two_at_zero = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_two)),
    );
    let moved = commit_items(
        &mut host,
        &store,
        replaced,
        77,
        range(source_position(0), after_two_at_one),
        vec![
            MutationPageItem::Object(ObjectChange::Move {
                target: target_two,
                object: SuccessorObject::new(id, ByteOffset::new(0), order_two, 17, 5),
            }),
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "x".into(),
            },
        ],
        MutationPositions::collapsed(after_two_at_zero),
        Vec::new(),
        1,
        1,
    );
    assert_marker(
        storage.clone(),
        &store,
        moved,
        marker_id,
        0,
        2,
        label,
        asset_id,
    );

    let before_two_at_zero = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order_two)),
    );
    let target =
        ObjectTarget::new(range(before_two_at_zero, after_two_at_zero), id, order_two).unwrap();
    let removed = commit_items(
        &mut host,
        &store,
        moved,
        78,
        target.range(),
        vec![MutationPageItem::Object(ObjectChange::Remove { target })],
        MutationPositions::collapsed(source_position(0)),
        Vec::new(),
        1,
        1,
    );
    assert_eq!(candidate_text(storage.clone(), &store, removed), b"x");
    assert_eq!(
        storage
            .draft_marker_identity(&store, removed.root(), marker_id)
            .unwrap(),
        None
    );
}

#[test]
fn marker_metadata_accepts_256_entries_as_one_bounded_widget_page() {
    let (_home, store, storage, thread) = fixture("marker-metadata-256", 201);
    let (mut host, base) = activated(storage.clone(), &store, thread, 202, 203);
    let key = mutation_key(base, 204);
    let zero = source_position(0);
    marker_readiness::begin_with_markers(
        &mut host,
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
        &bounded_marker_targets(256),
    )
    .unwrap();
    let (items, metadata) = marker_insertions(256, 0);
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        items,
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page),
            metadata.into_boxed_slice(),
        ),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(base, 204))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 256);
    assert_eq!(head.proposal().item_total(), 256);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
}

#[test]
fn marker_metadata_rejects_257_entries_before_frontier_or_storage_effect() {
    let (_home, store, storage, thread) = fixture("marker-metadata-257", 211);
    let (mut host, base) = activated(storage.clone(), &store, thread, 212, 213);
    let key = mutation_key(base, 214);
    let zero = source_position(0);
    marker_readiness::begin_with_markers(
        &mut host,
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
        &bounded_marker_targets(1),
    )
    .unwrap();
    let (mut items, metadata) = marker_insertions(257, 0);
    items.truncate(1);
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        items,
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(page.clone()),
            metadata.clone().into_boxed_slice(),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    let head = storage
        .draft_mutation_staging_head(&store, staging_identity(base, 214))
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
    assert_eq!(head.proposal().item_total(), 0);
    host.stage_mutation_page(
        &store,
        MutationPageRequest::new(page),
        Box::new([metadata[0].clone()]),
    )
    .unwrap();
}

fn marker_insertions(
    count: usize,
    anchor: u64,
) -> (Vec<MutationPageItem>, Vec<ComposerHostImageMarkerMetadata>) {
    let mut items = Vec::with_capacity(count);
    let mut metadata = Vec::with_capacity(count);
    for ordinal in 1..=u64::try_from(count).unwrap() {
        let id = bounded_marker_id(ordinal);
        items.push(MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                id,
                ByteOffset::new(anchor),
                InlineObjectOrder::new(u128::from(ordinal)),
                1,
                1,
            ),
        }));
        metadata.push(ComposerHostImageMarkerMetadata::new(
            id,
            asset_id_for_object(id),
        ));
    }
    (items, metadata)
}

fn bounded_marker_id(ordinal: u64) -> InlineObjectId {
    InlineObjectId::new(0xa100_0000_0000_0000_0000_0000_0000_0000_u128 + u128::from(ordinal))
}

fn bounded_marker_targets(count: u64) -> Vec<DraftPieceMarkerV1> {
    (1..=count)
        .map(|ordinal| {
            let id = bounded_marker_id(ordinal);
            DraftPieceMarkerV1::new(
                SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes()),
                ordinal,
                ImageLabelOrdinal::new(ordinal).unwrap(),
                asset_id_for_object(id),
            )
        })
        .collect()
}

#[test]
fn pure_rightward_marker_move_persists_at_its_successor_anchor() {
    let (_home, store, storage, thread) = fixture("rightward-marker-move", 85);
    let (mut host, base) = activated(storage.clone(), &store, thread, 86, 87);
    let text = commit_text(&mut host, &store, base, 88, 0, 0, "ab", 2, 1);
    let id = InlineObjectId::new(0x8181);
    let order = InlineObjectOrder::new(1);
    let label = ImageLabelOrdinal::new(1).unwrap();
    let asset_id = asset_id_for_object(id);
    let at_zero = source_position(0);
    let after_zero = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    );
    let inserted = commit_items(
        &mut host,
        &store,
        text,
        89,
        range(at_zero, at_zero),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 17, 5),
        })],
        MutationPositions::collapsed(after_zero),
        vec![labeled_marker_metadata(id, label, asset_id)],
        2,
        1,
    );
    let before_zero = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order)),
    );
    let target = ObjectTarget::new(range(before_zero, after_zero), id, order).unwrap();
    let after_two = SourcePosition::new(
        ByteOffset::new(2),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    );
    let moved = commit_items(
        &mut host,
        &store,
        inserted,
        90,
        target.range(),
        vec![MutationPageItem::Object(ObjectChange::Move {
            target,
            object: SuccessorObject::new(id, ByteOffset::new(2), order, 17, 5),
        })],
        MutationPositions::collapsed(after_two),
        Vec::new(),
        2,
        1,
    );

    assert_eq!(candidate_text(storage.clone(), &store, moved), b"ab");
    assert_marker(
        storage.clone(),
        &store,
        moved,
        SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes()),
        2,
        1,
        label,
        asset_id,
    );
}

#[test]
fn marker_effect_on_a_later_proposal_page_commits_from_durable_staging() {
    let (_home, store, storage, thread) = fixture("later-marker", 91);
    let (mut host, base) = activated(storage.clone(), &store, thread, 92, 93);
    let text = commit_text(&mut host, &store, base, 94, 0, 0, "ab", 2, 1);
    let key = mutation_key(text, 95);
    let start = source_position(0);
    let end = source_position(2);
    let id_value = u128::from_be_bytes([0x80, 9, 8, 7, 6, 5, 4, 3, 2, 1, 10, 11, 12, 13, 14, 0xfe]);
    let id = InlineObjectId::new(id_value);
    let order = InlineObjectOrder::new(7);
    let label = ImageLabelOrdinal::new(11).unwrap();
    let asset_id = asset_id_for_object(id);
    marker_readiness::begin_with_markers(
        &mut host,
        &store,
        text,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(start),
                range(start, end),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
        &[DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes(id_value.to_be_bytes()),
            7,
            label,
            asset_id,
        )],
    )
    .unwrap();

    let utf8 = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "AB".into(),
        }],
    )
    .unwrap();
    let prior = utf8.cumulative_identity();
    let totals = utf8.totals();
    host.stage_mutation_page(&store, MutationPageRequest::new(utf8), Box::new([]))
        .unwrap();

    let marker = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(1),
            1,
            prior,
        ),
        MutationCursor::new(2),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(1), order, 19, 5),
        })],
    )
    .unwrap();
    let marker_identity = marker.cumulative_identity();
    let totals = add_totals(totals, marker.totals());
    host.stage_mutation_page(
        &store,
        MutationPageRequest::new(marker),
        vec![ComposerHostImageMarkerMetadata::new(id, asset_id)].into_boxed_slice(),
    )
    .unwrap();

    let after = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order)),
    );
    host.finish_mutation_input(
        &store,
        MutationFinishInput::new(
            key,
            empty_finish(),
            MutationStreamFinish {
                next_cursor: MutationCursor::new(2),
                next_ordinal: 2,
                cumulative_identity: marker_identity,
                totals,
            },
            LogicalExtent::new(2, 1),
            MutationPositions::collapsed(after),
        ),
    )
    .unwrap();
    let committed = commit(&mut host, &store, key);
    assert_eq!(candidate_text(storage.clone(), &store, committed), b"AB");
    assert_marker(
        storage.clone(),
        &store,
        committed,
        SyndicDraftMarkerId::from_bytes(id_value.to_be_bytes()),
        1,
        7,
        label,
        asset_id,
    );
}

#[test]
fn stale_candidate_and_lower_operation_aba_elect_exact_durable_conflict() {
    let (_home, store, storage, thread) = fixture("stale-conflict", 81);
    let durable_before = current(storage.clone(), &store, thread);
    let (mut host, base) = activated(storage.clone(), &store, thread, 82, 83);
    let high_key = mutation_key(base, 87);
    let zero = source_position(0);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                high_key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(high_key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
    let drained = host.binding().unwrap();
    assert_eq!(drained.root(), base.root());
    assert_eq!(drained.history(), base.history());
    assert_eq!(drained.range_binding(), base.range_binding());
    assert!(drained.candidate().session_generation() > base.candidate().session_generation());
    let base = drained;
    let session = match storage
        .draft_editor_candidate_session(
            &store,
            base.candidate().draft_id(),
            base.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("candidate session was not active: {other:?}"),
    };
    let advance = transaction_for_session(
        storage.clone(),
        &store,
        session,
        88,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("advanced".into())],
        )],
        point(0),
    );
    run_transaction(storage.clone(), &store, &advance, 2);
    let advanced = match storage
        .draft_editor_candidate_session(
            &store,
            base.candidate().draft_id(),
            base.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("advanced candidate session was not active: {other:?}"),
    };
    assert_ne!(advanced.newest_root(), base.root());

    let key = mutation_key(base, 85);
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(zero),
                range(zero, zero),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    host.dispose_composer_service(&store).unwrap();
    assert!(matches!(
        host.execute_mutation(
            &store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::MutationNotPending)
    ));
    assert_eq!(host.binding(), None);
    let after = match storage
        .draft_editor_candidate_session(
            &store,
            base.candidate().draft_id(),
            base.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("session changed lifecycle after conflict: {other:?}"),
    };
    assert_eq!(after, advanced);
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, staging_identity(base, 85))
            .unwrap(),
        DraftMutationStagingStatusV1::Conflict { .. }
    ));
    assert_eq!(current(storage.clone(), &store, thread), durable_before);
}
