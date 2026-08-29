use super::*;

struct DeepFixture {
    fixture: Fixture,
    root: SyndicTurnId,
    middle: SyndicTurnId,
    tail: SyndicTurnId,
    tail_digest: beryl_model::SyndicPathDigest,
}

fn deep_fixture() -> DeepFixture {
    let mut fixture = promotion_fixture(94, id(94));
    let tail = fixture
        .records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Thread(thread) if thread.id() == fixture.thread => {
                thread.committed_tail()
            }
            _ => None,
        })
        .unwrap();
    let root = SyndicTurnId::from_bytes([160; 16]);
    let middle = SyndicTurnId::from_bytes([161; 16]);
    let root_digest = root_turn_chain_digest(root);
    let middle_digest = child_turn_chain_digest(middle, root, root_digest);
    let tail_digest = child_turn_chain_digest(tail, middle, middle_digest);

    fixture.records.retain(|record| {
        !matches!(
            record,
            FixtureRecord::TranscriptBuild(_) | FixtureRecord::TranscriptPathTurn(_)
        )
    });
    for record in &mut fixture.records {
        match record {
            FixtureRecord::Thread(thread) if thread.id() == fixture.thread => {
                *thread = ThreadRecord::new(
                    thread.id(),
                    SelectedPathProof::new(Some(tail), thread.revision(), tail_digest),
                    thread.current_draft_id(),
                    thread.lineage(),
                    thread.context_owner_id(),
                );
            }
            FixtureRecord::Turn(turn) if turn.id() == tail => {
                *turn = TurnRecord::new(
                    turn.id(),
                    turn.origin_thread_id(),
                    turn.kind(),
                    ConversationParent::Turn(middle),
                    Some(middle),
                    TurnDepth::new(3).unwrap(),
                    tail_digest,
                    turn.submitted_at(),
                );
            }
            FixtureRecord::TranscriptViewHead(head) if head.thread_id() == fixture.thread => {
                *head = TranscriptViewHeadRecord::new(
                    head.thread_id(),
                    head.generation(),
                    head.revision(),
                    head.entry_count(),
                    Some(tail),
                    tail_digest,
                    head.lifecycle(),
                );
            }
            FixtureRecord::HistorySummary(summary) if summary.thread_id() == fixture.thread => {
                *summary = HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next().unwrap(),
                    summary.thread_revision(),
                    Some(tail),
                    tail_digest,
                    summary.complete(),
                    summary.last_activity_at(),
                );
            }
            FixtureRecord::Binding(binding) if binding.thread_id() == fixture.thread => {
                let selected = binding.selected_path();
                *binding = BindingRecord::new(
                    binding.thread_id(),
                    binding.revision(),
                    SelectedPathProof::new(Some(tail), selected.thread_revision(), tail_digest),
                    binding.state().clone(),
                );
            }
            FixtureRecord::BindingHead(head) if head.thread_id() == fixture.thread => {
                *head = BindingHeadRecord::new(
                    head.thread_id(),
                    head.revision(),
                    head.lifecycle(),
                    tail_digest,
                );
            }
            _ => {}
        }
    }

    fixture.records.extend([
        FixtureRecord::Turn(TurnRecord::new(
            root,
            fixture.thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            root_digest,
            timestamp(0),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            root,
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            timestamp(1),
        )),
        terminal_event(root),
        FixtureRecord::Turn(TurnRecord::new(
            middle,
            fixture.thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Turn(root),
            Some(root),
            TurnDepth::new(2).unwrap(),
            middle_digest,
            timestamp(1),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            middle,
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            timestamp(2),
        )),
        terminal_event(middle),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            root,
            middle,
            TurnDepth::new(2).unwrap(),
            middle_digest,
        )),
        FixtureRecord::TurnChild(TurnChildIndexRecord::new(
            middle,
            tail,
            TurnDepth::new(3).unwrap(),
            tail_digest,
        )),
    ]);
    fixture.records.extend(item_free_transcript_build_records(
        fixture.thread,
        beryl_model::ThreadRevision::new(2).unwrap(),
        &[
            (root, root_digest, TurnLifecycle::Failed, 1, timestamp(1)),
            (
                middle,
                middle_digest,
                TurnLifecycle::Failed,
                1,
                timestamp(2),
            ),
            (tail, tail_digest, TurnLifecycle::Failed, 1, timestamp(5)),
        ],
    ));
    DeepFixture {
        fixture,
        root,
        middle,
        tail,
        tail_digest,
    }
}

fn terminal_event(turn: SyndicTurnId) -> FixtureRecord {
    FixtureRecord::SourceEvent(
        SourceEventRecord::new(
            turn,
            SourceEventSequence::FIRST,
            None,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
            ),
        )
        .unwrap(),
    )
}

#[test]
fn promotion_after_a_deep_tail_derives_child_digest_depth_and_deterministic_skip() {
    let deep = deep_fixture();
    let (home, store, storage) = seed("phase58-promotion-deep-tail", deep.fixture.records.clone());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let current_tail = storage.turn(&store, deep.tail, limit()).unwrap().unwrap();
    assert_eq!(current_tail.depth(), TurnDepth::new(3).unwrap());
    assert_eq!(current_tail.parent(), ConversationParent::Turn(deep.middle));
    assert_eq!(current_tail.ancestor_skip(), Some(deep.middle));

    let request = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([162; 16]),
        SyndicItemId::from_bytes([163; 16]),
    );
    match execute_promotion(&store, storage, request.clone()) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected deep-tail promotion to commit without later failure, got {outcome:?}")
        }
    }
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact,
    );

    let expected_digest =
        child_turn_chain_digest(request.successor_turn_id(), deep.tail, deep.tail_digest);
    let successor = storage
        .turn(&store, request.successor_turn_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(successor.parent(), ConversationParent::Turn(deep.tail));
    assert_eq!(successor.depth(), TurnDepth::new(4).unwrap());
    assert_eq!(successor.chain_digest(), expected_digest);
    assert_eq!(successor.ancestor_skip(), Some(deep.root));

    let children = storage
        .turn_children(&store, deep.tail, None, page_limits())
        .unwrap();
    let child = children
        .records()
        .iter()
        .find(|child| child.child_id() == request.successor_turn_id())
        .unwrap();
    assert_eq!(child.child_depth(), TurnDepth::new(4).unwrap());
    assert_eq!(child.child_digest(), expected_digest);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact,
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn reconciliation_rejects_a_deep_successor_with_a_malformed_ancestor_skip() {
    let deep = deep_fixture();
    let (_home, store, storage) = seed(
        "phase58-promotion-malformed-successor-skip",
        deep.fixture.records,
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let request = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([164; 16]),
        SyndicItemId::from_bytes([165; 16]),
    );
    match execute_promotion(&store, storage, request.clone()) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected deep-tail promotion to commit without later failure, got {outcome:?}")
        }
    }
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact,
    );

    let successor = storage
        .turn(&store, request.successor_turn_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(successor.ancestor_skip(), Some(deep.root));
    let malformed = TurnRecord::new(
        successor.id(),
        successor.origin_thread_id(),
        successor.kind(),
        successor.parent(),
        Some(deep.middle),
        successor.depth(),
        successor.chain_digest(),
        successor.submitted_at(),
    );
    commit(&store, storage, batch([FixtureRecord::Turn(malformed)]));

    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision,
    );
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("child turn depth, ancestor skip, or chain digest is invalid"),
        "unexpected validation error: {error}",
    );
    store.close().unwrap();
}
