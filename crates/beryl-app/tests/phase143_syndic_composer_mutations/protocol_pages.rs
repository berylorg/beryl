use super::*;

#[test]
fn typing_newline_paste_delete_and_cut_use_one_paged_protocol() {
    let (_home, store, storage, thread) = fixture("phase149-text", 1);
    let durable_before = current(storage, &store, thread);
    let (mut host, base) = activated(storage, &store, thread, 2, 3);

    let typed = commit_text(&mut host, &store, base, 4, 0, 0, "hello\n", 6, 2);
    assert_eq!(candidate_text(storage, &store, typed), b"hello\n");

    let pasted = commit_text(&mut host, &store, typed, 5, 6, 6, "世界!", 13, 2);
    assert_eq!(
        candidate_text(storage, &store, pasted),
        "hello\n世界!".as_bytes()
    );

    let deleted = commit_text(&mut host, &store, pasted, 6, 6, 12, "", 7, 2);
    assert_eq!(candidate_text(storage, &store, deleted), b"hello\n!");

    let cut = commit_text(&mut host, &store, deleted, 7, 0, 5, "", 2, 2);
    assert_eq!(candidate_text(storage, &store, cut), b"\n!");
    assert_eq!(host.binding(), Some(cut));
    assert_ne!(cut.history(), base.history());
    assert_eq!(current(storage, &store, thread), durable_before);
}

#[test]
fn more_than_256_pages_commit_once_without_coordinator_growth() {
    let (_home, store, storage, thread) = fixture("phase149-large", 21);
    let (mut host, base) = activated(storage, &store, thread, 22, 23);
    let key = mutation_key(base, 24);
    let zero = source_position(0);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(zero),
        range(zero, zero),
        0,
    );
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0)),
    )
    .unwrap();

    let mut cursor = MutationCursor::new(0);
    let mut prior = MutationIdentity::ROOT;
    let mut totals = MutationTotals::default();
    for ordinal in 0..258_u64 {
        let page = MutationPage::new(
            MutationPageKey::new(key, MutationLane::Proposal, cursor, ordinal, prior),
            MutationCursor::new(ordinal + 1),
            vec![MutationPageItem::Utf8 {
                inserted_offset: ordinal,
                text: "x".into(),
            }],
        )
        .unwrap();
        cursor = page.next_cursor();
        prior = page.cumulative_identity();
        totals = add_totals(totals, page.totals());
        assert!(matches!(
            host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([])),
            Ok(MutationPageAcceptance::Accepted { .. })
        ));
        assert_eq!(host.pending_request_count(), 0);
        assert_eq!(
            host.mutation_status(),
            Some(ComposerHostMutationStatus::Admitted)
        );
    }
    let finish = finish_input(
        key,
        empty_finish(),
        MutationStreamFinish {
            next_cursor: cursor,
            next_ordinal: 258,
            cumulative_identity: prior,
            totals,
        },
        258,
        1,
    );
    host.finish_mutation_input(&store, finish).unwrap();
    let binding = commit(&mut host, &store, key);
    assert_eq!(candidate_text(storage, &store, binding), vec![b'x'; 258]);
}

#[test]
fn lane_order_replay_wrong_cursor_collision_and_backpressure_are_exact() {
    let (_home, store, storage, thread) = fixture("phase149-order", 41);
    let (mut host, base) = activated(storage, &store, thread, 42, 43);
    let key = mutation_key(base, 44);
    let zero = source_position(0);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(zero),
        range(zero, zero),
        0,
    );
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0)),
    )
    .unwrap();
    assert!(matches!(
        host.begin_mutation(
            &store,
            base,
            MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0))
        ),
        Err(ComposerHostError::MutationPending)
    ));

    let wrong = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(9),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(10),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "x".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(wrong), Box::new([])),
        Err(ComposerHostError::MutationMalformed)
    ));

    let page = MutationPage::new(
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
            text: "x".into(),
        }],
    )
    .unwrap();
    let replay = page.clone();
    host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    assert_eq!(
        host.stage_mutation_page(
            &store,
            MutationPageRequest::new(replay.clone()),
            Box::new([]),
        )
        .unwrap(),
        MutationPageAcceptance::Replay
    );

    let collision = MutationPage::new(
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
            text: "different".into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(collision), Box::new([])),
        Err(ComposerHostError::MutationIdentityCollision)
    ));

    let second = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(1),
            1,
            replay.cumulative_identity(),
        ),
        MutationCursor::new(2),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 1,
            text: "y".into(),
        }],
    )
    .unwrap();
    host.stage_mutation_page(&store, MutationPageRequest::new(second), Box::new([]))
        .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(replay), Box::new([])),
        Err(ComposerHostError::StaleRequestIdentity)
    ));
}

#[test]
fn one_widget_page_admits_the_maximum_257_physical_pages_atomically_and_releases_payload() {
    let (_home, store, storage, thread) = fixture("phase153-max-batch", 51);
    let (mut host, base) = activated(storage, &store, thread, 52, 53);
    let key = mutation_key(base, 54);
    let zero = source_position(0);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(zero),
        range(zero, zero),
        0,
    );
    host.begin_mutation(
        &store,
        base,
        MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0)),
    )
    .unwrap();
    let mut items = Vec::with_capacity(256);
    items.push(MutationPageItem::Utf8 {
        inserted_offset: 0,
        text: "a".repeat(65_536).into(),
    });
    items.extend((1..256).map(|_| MutationPageItem::Utf8 {
        inserted_offset: 65_536,
        text: String::new().into(),
    }));
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
    let payload = page.clone();
    assert_eq!(payload.payload_owner_count(), 2);
    host.stage_mutation_page(&store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    assert_eq!(payload.payload_owner_count(), 1);
    let identity = staging_identity(base, 54);
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 257);
    assert_eq!(head.proposal().item_total(), 257);
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
    assert_eq!(host.binding(), Some(base));
}

#[test]
fn page_limits_reject_before_frontier_or_storage_effect() {
    let (_home, store, storage, thread) = fixture("phase153-page-limits", 55);
    let (mut host, base) = activated(storage, &store, thread, 56, 57);
    let key = mutation_key(base, 58);
    let zero = source_position(0);
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
    let oversized = MutationPage::new(
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
            text: "x".repeat(65_537).into(),
        }],
    )
    .unwrap();
    assert!(matches!(
        host.stage_mutation_page(&store, MutationPageRequest::new(oversized), Box::new([])),
        Err(ComposerHostError::MutationMalformed)
    ));
    let identity = staging_identity(base, 58);
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(head.proposal().next_cursor(), 0);
    assert_eq!(head.proposal().item_total(), 0);

    let valid = MutationPage::new(
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
            text: "ok".into(),
        }],
    )
    .unwrap();
    host.stage_mutation_page(&store, MutationPageRequest::new(valid), Box::new([]))
        .unwrap();
}

#[test]
fn operation_highwater_and_lane_receipts_reset_on_rebind_release_and_fresh_session() {
    let (_home, store, storage, thread) = fixture("phase155-binding-highwater", 61);
    let (mut host, base) = activated(storage, &store, thread, 62, 63);
    begin_then_cancel(&mut host, &store, base, 100);

    assert!(host.release().unwrap());
    let rebound = reactivate(&mut host, &store, thread, 62, 63);
    begin_then_cancel(&mut host, &store, rebound, 1);

    assert!(host.release().unwrap());
    let fresh = reactivate(&mut host, &store, thread, 65, 66);
    begin_then_cancel(&mut host, &store, fresh, 1);
}

fn begin_then_cancel(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
) {
    let zero = source_position(0);
    let key = mutation_key(binding, operation);
    host.begin_mutation(
        store,
        binding,
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
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert_eq!(
        host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &cancellation,
        )
        .unwrap(),
        ComposerHostMutationOutcome::Cancelled
    );
}
