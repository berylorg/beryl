use super::*;
use beryl_home_store::WholeHomeScrubTrigger;

#[test]
fn generated_history_validates_bounded_maximum_text_and_rejects_receipt_corruption() {
    let home = support::TestHome::new("generated-history");
    let mut candidate = support::open(home.path());
    let mut storage = SyndicStorage::register(&mut candidate).unwrap();
    let mut store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    support::populated::seed_populated(&store, storage.clone());
    support::converge_and_release_terminal_history(
        &store,
        storage.clone(),
        support::id(30),
        support::populated::source_turn(),
    );
    let request = support::discussion_handoff::active_request(
        &store,
        &storage,
        ResolutionIntentId::from_bytes([231; 16]),
        JobId::from_bytes([231; 16]),
    );
    let parent = request.parent.thread_id;
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let admission = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let pending = admission.intent().new_gate();
    support::discussion_input::committed(&store, admission.contribution());
    let text = "🦀".repeat(65_536);
    let item_id = SyndicItemId::from_bytes([232; 16]);
    let turn_id = support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        parent,
        SyndicDraftId::from_bytes([233; 16]),
        item_id,
        &format!("Discussion resolution:\n\n{text}"),
        support::timestamp(100),
    );
    let turn = storage.turn(&store, turn_id, limit).unwrap().unwrap();
    let item = storage
        .canonical_item(&store, item_id, limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(&store, parent, limit).unwrap().unwrap();
    let proof = DiscussionHandoffReceipt {
        parent_thread_revision: request.parent.thread_revision,
        parent_gate_revision: request.parent.input_gate_revision,
        child_thread_id: request.thread_id,
        intent_id: request.intent_id,
        job_id: request.job_id,
        context_owner: request.context_owner,
        context_digest: request.context_digest,
        resolving_turn_id: request.resolving_target.pending().active_turn_id(),
        resolution_digest: Sha256::digest(text.as_bytes()).into(),
        parent_turn_id: turn_id,
        canonical_item_id: item_id,
    };
    let input = AcceptedInputRecord::new(
        SyndicAcceptedInputId::from_bytes([231; 16]),
        parent,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputSource::DiscussionHandoff(proof),
        item.presentation().content().unwrap(),
        None,
        turn.submitted_at(),
    )
    .unwrap();
    let generated = CanonicalItemRecord::local_discussion_handoff(
        item_id,
        turn_id,
        item.ordinal(),
        item.revision(),
        input.content(),
        input.id(),
    );
    let batch = support::batch([
        FixtureRecord::Turn(TurnRecord::new(
            turn_id,
            parent,
            TurnKind::BerylDiscussionHandoff,
            turn.parent(),
            turn.ancestor_skip(),
            turn.depth(),
            turn.chain_digest(),
            turn.submitted_at(),
        )),
        FixtureRecord::CanonicalItem(generated.clone()),
        FixtureRecord::AcceptedInput(input.clone()),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::from_source(
            parent,
            input.ordinal(),
            input.id(),
            AcceptedOrderSource::DiscussionHandoff,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                parent,
                gate.revision(),
                gate.state().clone(),
                1,
                gate.route_generation_high_water(),
                gate.selected_route(),
                0,
                0,
                0,
            )
            .unwrap(),
        ),
    ]);
    support::commit(&store, storage.clone(), batch);
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let attributes = storage
        .thread_attributes(&store, request.thread_id, limit)
        .unwrap()
        .unwrap();
    let release = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: pending,
                attributes_revision: attributes.revision(),
                archived_at: support::timestamp(101),
            },
        )
        .unwrap();
    support::discussion_input::committed(&store, release.contribution());
    let current = storage
        .current_draft(&store, parent, limit)
        .unwrap()
        .unwrap();
    let thread = current.thread();
    let revision = thread.revision().checked_next().unwrap();
    let history = storage
        .history_summary(&store, parent, limit)
        .unwrap()
        .unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                parent,
                history.revision().checked_next().unwrap(),
                revision,
                thread.committed_tail(),
                thread.selected_path_digest(),
                false,
                history.last_activity_at(),
            )),
            FixtureRecord::Thread(ThreadRecord::new(
                parent,
                SelectedPathProof::new(
                    thread.committed_tail(),
                    revision,
                    thread.selected_path_digest(),
                ),
                thread.current_draft_id(),
                thread.lineage(),
                thread.context_owner_id(),
            )),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                parent,
                current.draft().id(),
                current.draft().revision(),
                revision,
            )),
        ]),
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    for (label, corrupted) in [
        (
            "digest",
            DiscussionHandoffReceipt {
                resolution_digest: [0; 32],
                ..proof
            },
        ),
        (
            "context",
            DiscussionHandoffReceipt {
                context_digest: DiscussionContextDigest::from_bytes([0; 32]),
                ..proof
            },
        ),
        (
            "item",
            DiscussionHandoffReceipt {
                canonical_item_id: SyndicItemId::from_bytes([234; 16]),
                ..proof
            },
        ),
        (
            "turn",
            DiscussionHandoffReceipt {
                parent_turn_id: SyndicTurnId::from_bytes([235; 16]),
                ..proof
            },
        ),
    ] {
        let corrupt = AcceptedInputRecord::new(
            input.id(),
            parent,
            input.ordinal(),
            AcceptedInputSource::DiscussionHandoff(corrupted),
            input.content(),
            None,
            input.admitted_at(),
        )
        .unwrap();
        support::commit(
            &store,
            storage.clone(),
            support::batch([FixtureRecord::AcceptedInput(corrupt)]),
        );
        assert!(
            store
                .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                .is_err(),
            "{label}"
        );
        let candidate = store.recover_same_home().unwrap();
        storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        store = candidate.publish().unwrap();
        support::commit(
            &store,
            storage.clone(),
            support::batch([FixtureRecord::AcceptedInput(input.clone())]),
        );
    }
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    for (label, text, digest) in [
        (
            "prefix",
            "Invalid discussion prefix but enough bytes".to_owned(),
            proof.resolution_digest,
        ),
        (
            "scalars",
            format!("Discussion resolution:\n\n{}", "a".repeat(65_537)),
            Sha256::digest("a".repeat(65_537).as_bytes()).into(),
        ),
    ] {
        let (content, mut records) = support::composer_content_records(
            &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
        );
        let corrupt = AcceptedInputRecord::new(
            input.id(),
            parent,
            input.ordinal(),
            AcceptedInputSource::DiscussionHandoff(DiscussionHandoffReceipt {
                resolution_digest: digest,
                ..proof
            }),
            content,
            None,
            input.admitted_at(),
        )
        .unwrap();
        records.push(FixtureRecord::AcceptedInput(corrupt));
        records.push(FixtureRecord::CanonicalItem(
            CanonicalItemRecord::local_discussion_handoff(
                item_id,
                turn_id,
                item.ordinal(),
                item.revision(),
                content,
                input.id(),
            ),
        ));
        support::commit(&store, storage.clone(), support::batch(records));
        let error = store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap_err();
        assert!(
            error.to_string().contains(if label == "prefix" {
                "generated input prefix"
            } else {
                "generated resolution scalar"
            }),
            "{error}"
        );
        let candidate = store.recover_same_home().unwrap();
        storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        store = candidate.publish().unwrap();
        support::commit(
            &store,
            storage.clone(),
            support::batch([
                FixtureRecord::AcceptedInput(input.clone()),
                FixtureRecord::CanonicalItem(generated.clone()),
            ]),
        );
    }
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::AcceptedRouteLeaf(
            AcceptedRouteLeafRecord::new(
                input.id(),
                parent,
                AcceptedRouteGeneration::FIRST,
                input.ordinal(),
                AcceptedInputRevision::new(1).unwrap(),
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Admitted,
            ),
        )]),
    );
    let error = store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("generated input parent revisions, order or route authority"),
        "{error}"
    );
    let candidate = store.recover_same_home().unwrap();
    storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    store = candidate.publish().unwrap();
    let mut cleanup = FixtureBatch::new();
    cleanup
        .delete(FixtureDelete::AcceptedRouteLeaf(input.id()))
        .unwrap();
    support::commit(&store, storage.clone(), cleanup);
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
