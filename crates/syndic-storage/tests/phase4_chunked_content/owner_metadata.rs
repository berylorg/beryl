use super::*;

#[test]
fn accepted_and_canonical_owners_remain_small_metadata_records() {
    let home = TestHome::new("phase4-small-owners");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(10);
    let draft = draft_id(11);
    let input = SyndicAcceptedInputId::from_bytes([12; 16]);
    let turn = SyndicTurnId::from_bytes([13; 16]);
    let item = SyndicItemId::from_bytes([14; 16]);
    let revision = AcceptedInputRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let ordinal = AcceptedInputOrdinal::FIRST;
    let generation = AcceptedRouteGeneration::FIRST;
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("large ".repeat(200_000)).unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let digest = syndic_storage::root_turn_chain_digest(turn);

    let mut records = empty_thread_records(thread, draft);
    let thread_revision = ThreadRevision::new(2).unwrap();
    for record in &mut records {
        match record {
            FixtureRecord::Thread(record) => {
                *record = ThreadRecord::new(
                    record.id(),
                    SelectedPathProof::new(
                        record.committed_tail(),
                        thread_revision,
                        record.selected_path_digest(),
                    ),
                    record.current_draft_id(),
                    record.lineage(),
                    record.image_label_frontiers(),
                    record.context_owner_id(),
                );
            }
            FixtureRecord::DraftByThread(index) => {
                *index = DraftByThreadRecord::new(
                    index.thread_id(),
                    index.draft_id(),
                    index.draft_revision(),
                    thread_revision,
                );
            }
            FixtureRecord::HistorySummary(summary) => {
                *summary = HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next().unwrap(),
                    thread_revision,
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    summary.complete(),
                    summary.last_activity_at(),
                );
            }
            _ => {}
        }
    }
    records.retain(|record| !matches!(record, FixtureRecord::InputGate(_)));
    records.extend(content_records);
    records.extend([
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input,
                thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    ThreadRevision::new(1).unwrap(),
                    SyndicDraftId::from_bytes(*input.as_bytes()),
                    DraftRevision::new(1).unwrap(),
                    InputGateRevision::new(1).unwrap(),
                    draft,
                )
                .unwrap(),
                generation,
                content,
                None,
                timestamp(1),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread, ordinal, input, generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
                AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                Some(ordinal),
                Some(ordinal),
                1,
                0,
                0,
                1,
                0,
                content.summary().logical_utf8_bytes(),
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input,
            thread,
            generation,
            ordinal,
            revision,
            AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            generation,
            AcceptedRouteRevision::FIRST,
            ordinal,
            ordinal,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(2).unwrap(),
                InputGateState::Idle,
                1,
                Some(generation),
                None,
                0,
                1,
                content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            syndic_storage::ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(
            TurnStateRecord::with_capture_frontiers(
                turn,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                0,
                1,
                1,
                1,
                0,
                Some(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Interrupted,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
                timestamp(2),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            projection_revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            projection_revision,
        )),
    ]);
    commit(&store, storage, batch(records));
    project_item(&store, storage, item);
    store.validate_registered_domains().unwrap();

    let accepted = storage
        .accepted_input(&store, input, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(accepted.content(), content);
    let canonical = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(canonical.presentation_content(), Some(content));
    assert!(content.summary().encoded_bytes() > 1_000_000);
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
