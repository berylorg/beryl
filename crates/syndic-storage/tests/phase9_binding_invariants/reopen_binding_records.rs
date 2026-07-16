use super::*;

fn pending_root_seed() -> FixtureBatch {
    let thread = id(50);
    let draft = draft_id(51);
    let turn = SyndicTurnId::from_bytes([52; 16]);
    let digest = root_turn_chain_digest(turn);
    let revision = beryl_model::ThreadRevision::new(1).unwrap();
    let mut records = thread_records_with_activity(thread, draft, Some(turn), digest, timestamp(2));
    records.retain(|record| {
        !matches!(
            record,
            FixtureRecord::HistorySummary(_) | FixtureRecord::InputGate(_)
        )
    });
    records.extend([
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                InputGateRevision::new(1).unwrap(),
                InputGateState::PendingTurn(turn),
                0,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            revision,
            Some(turn),
            digest,
            false,
            timestamp(2),
        )),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            timestamp(2),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            0,
            timestamp(2),
        )),
    ]);
    records.extend(item_free_transcript_build_records(
        thread,
        revision,
        &[(turn, digest, TurnLifecycle::Pending, 0, timestamp(2))],
    ));
    batch(records)
}

#[test]
fn reopen_requires_creation_time_unbound_binding_revision() {
    let thread = id(55);
    let draft = draft_id(56);
    let selected = SelectedPathProof::new(
        None,
        beryl_model::ThreadRevision::new(1).unwrap(),
        empty_selected_path_digest(),
    );
    let represented =
        CasRepresentedPrefixProof::new(None, selected.thread_revision(), selected.digest());
    let cas_thread = CasThreadId::new("corrupt-creation-binding").unwrap();
    let revision = BindingRevision::new(1).unwrap();
    exercise_case(
        "phase9-creation-binding-state",
        "initial binding is not the creation-time unbound revision",
        || batch(empty_thread_records(thread, draft)),
        || {
            batch([
                FixtureRecord::Binding(BindingRecord::new(
                    thread,
                    revision,
                    selected,
                    BindingState::valid(UsableCasBinding::new(
                        execution_binding(),
                        cas_thread.clone(),
                        represented,
                        beryl_model::CasNativeTurnCount::ZERO,
                        test_tool_profile(),
                        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
                    )),
                )),
                FixtureRecord::BindingHead(BindingHeadRecord::new(
                    thread,
                    revision,
                    BindingLifecycle::Valid,
                    selected.digest(),
                )),
                FixtureRecord::CasThread(CasThreadIndexRecord::new(
                    cas_thread.clone(),
                    thread,
                    revision,
                )),
                FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                    cas_thread.clone(),
                    thread,
                    revision,
                )),
            ])
        },
    );
}

fn pending_claim_corruption() -> FixtureBatch {
    let thread = id(50);
    let turn = SyndicTurnId::from_bytes([52; 16]);
    let revision = beryl_model::ThreadRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(2).unwrap();
    let digest = root_turn_chain_digest(turn);
    let selected = SelectedPathProof::new(Some(turn), revision, digest);
    let represented = CasRepresentedPrefixProof::new(Some(turn), revision, digest);
    let cas_thread = CasThreadId::new("corrupt-pending-claim").unwrap();
    let usable = UsableCasBinding::new(
        execution_binding(),
        cas_thread.clone(),
        represented,
        beryl_model::CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Continuation, represented).unwrap(),
    );
    batch([
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::valid(usable),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Valid,
            digest,
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::new(
            cas_thread.clone(),
            thread,
            binding_revision,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            cas_thread,
            thread,
            binding_revision,
        )),
    ])
}

#[test]
fn reopen_rejects_a_persisted_binding_that_claims_a_pending_turn() {
    exercise_case(
        "phase9-persisted-pending-claim",
        "pending binding does not represent exactly its parent prefix",
        pending_root_seed,
        pending_claim_corruption,
    );
}
