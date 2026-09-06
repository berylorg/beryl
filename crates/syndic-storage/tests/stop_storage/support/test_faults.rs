use super::*;

#[cfg(feature = "test-faults")]
pub fn admit_current_draft_as_accepted(
    fixture: &ActiveStopFixture,
    text: &str,
    next_draft_byte: u8,
    at: u64,
) -> syndic_storage::AcceptedInputRecord {
    admit_queued_text(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        text,
        SyndicDraftId::from_bytes([next_draft_byte; 16]),
        at,
    )
}

#[cfg(feature = "test-faults")]
pub fn admit_queued_text(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    text: &str,
    next_draft_id: SyndicDraftId,
    at: u64,
) -> syndic_storage::AcceptedInputRecord {
    use crate::support::exact_cas;
    use beryl_model::{AcceptedInputRevision, DraftRevision};
    use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
    use syndic_storage::{
        AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal,
        AcceptedInputRecord, AcceptedNextSourceRecord, AcceptedOrderIndexRecord,
        AcceptedReadySourceRecord, AcceptedRouteGenerationHeadRecord,
        AcceptedRouteGenerationRecord, AcceptedRouteHeadProof, AcceptedRouteLeafRecord,
        AcceptedRouteLeafState, AcceptedRouteRevision, AcceptedRouteTarget, DraftByThreadRecord,
        DraftRecord, DraftSubmissionIntent, HistorySummaryRecord, InputGateRecord, InputGateState,
        NextTurnReason, SelectedPathProof, ThreadRecord,
    };

    let mut auxiliary_thread_bytes = *next_draft_id.as_bytes();
    for byte in &mut auxiliary_thread_bytes {
        *byte ^= 0x3c;
    }
    let auxiliary_thread = SyndicThreadId::from_bytes(auxiliary_thread_bytes);
    let mut auxiliary_draft_bytes = *next_draft_id.as_bytes();
    for byte in &mut auxiliary_draft_bytes {
        *byte ^= 0x5a;
    }
    let auxiliary_draft = SyndicDraftId::from_bytes(auxiliary_draft_bytes);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                auxiliary_thread,
                auxiliary_draft,
                exact_cas::execution_binding(),
                timestamp(at),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    let mut auxiliary_next_draft_bytes = *next_draft_id.as_bytes();
    for byte in &mut auxiliary_next_draft_bytes {
        *byte ^= 0xa5;
    }
    let auxiliary_next_draft = SyndicDraftId::from_bytes(auxiliary_next_draft_bytes);
    let mut auxiliary_item_bytes = *next_draft_id.as_bytes();
    for byte in &mut auxiliary_item_bytes {
        *byte ^= 0xc3;
    }
    let auxiliary_item = SyndicItemId::from_bytes(auxiliary_item_bytes);
    exact_cas::submit_current_draft(
        store,
        storage.clone(),
        auxiliary_thread,
        auxiliary_next_draft,
        auxiliary_item,
        text,
        timestamp(at),
    );
    let content = storage
        .canonical_item(store, auxiliary_item, point_limit())
        .unwrap()
        .unwrap()
        .presentation_content()
        .unwrap();

    let current = storage
        .current_draft(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let ordinal_value = gate.accepted_high_water().checked_add(1).unwrap();
    let ordinal = AcceptedInputOrdinal::new(ordinal_value).unwrap();
    let extends_selected = matches!(
        gate.state(),
        InputGateState::AwaitingSteering(_) | InputGateState::Steerable(_)
    );
    let current_generation = if extends_selected {
        let proof = gate.selected_route().unwrap();
        Some(
            syndic_storage::test_faults::accepted_route_generation(
                store,
                storage.clone(),
                thread_id,
                proof.generation(),
            )
            .unwrap(),
        )
    } else {
        None
    };
    let generation_id = current_generation.as_ref().map_or_else(
        || {
            gate.route_generation_high_water()
                .map_or(syndic_storage::AcceptedRouteGeneration::FIRST, |value| {
                    value.checked_next().unwrap()
                })
        },
        AcceptedRouteGenerationRecord::generation,
    );
    let generation_revision = current_generation
        .as_ref()
        .map_or(AcceptedRouteRevision::FIRST, |generation| {
            generation.revision().checked_next().unwrap()
        });
    let next_reason = match gate.state() {
        InputGateState::PendingTurn(_) => Some(NextTurnReason::PendingTurn),
        InputGateState::Compacting { .. } => Some(NextTurnReason::Compaction),
        InputGateState::Stopping { .. } => {
            let operation = storage
                .stop_operation(
                    store,
                    StopOperationId::new(thread_id, gate.state().stop_operation_nonce().unwrap()),
                    point_limit(),
                )
                .unwrap()
                .unwrap();
            Some(if operation.admission().is_provider_operation() {
                NextTurnReason::Compaction
            } else {
                NextTurnReason::Stop
            })
        }
        InputGateState::FinalizingHistory(_) => Some(NextTurnReason::TerminalHistory),
        InputGateState::AwaitingTerminal(_) => Some(NextTurnReason::UnknownTerminal),
        InputGateState::AwaitingSteering(_) | InputGateState::Steerable(_) => None,
        InputGateState::Idle => panic!("queued fixture requires a non-idle gate"),
    };
    let input_count = current_generation
        .as_ref()
        .map_or(0, AcceptedRouteGenerationRecord::input_count)
        + 1;
    let first_ordinal = current_generation
        .as_ref()
        .and_then(AcceptedRouteGenerationRecord::first_ordinal)
        .or(Some(ordinal));
    let logical_bytes = content.summary().logical_utf8_bytes();
    let route = AcceptedRouteGenerationRecord::new(
        thread_id,
        generation_id,
        generation_revision,
        current_generation.as_ref().map_or_else(
            || AcceptedRouteTarget::NextTurn(next_reason.unwrap()),
            |generation| generation.target().clone(),
        ),
        first_ordinal,
        Some(ordinal),
        input_count,
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::ready_retryable_count)
            + u64::from(next_reason.is_none()),
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::delivering_count),
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::next_turn_count)
            + u64::from(next_reason.is_some()),
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::terminal_count),
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::live_logical_utf8_bytes)
            + logical_bytes,
        current_generation.as_ref().map_or(
            0,
            AcceptedRouteGenerationRecord::delivering_logical_utf8_bytes,
        ),
    );
    let route = route.unwrap();
    let route_proof = AcceptedRouteHeadProof::new(generation_id, generation_revision);
    let gate_revision = gate.revision().checked_next().unwrap();
    let next_gate = InputGateRecord::new(
        thread_id,
        gate_revision,
        gate.state().clone(),
        ordinal_value,
        if current_generation.is_some() {
            gate.route_generation_high_water()
        } else {
            Some(generation_id)
        },
        if extends_selected {
            Some(route_proof)
        } else {
            gate.selected_route()
        },
        gate.live_steering_count() + u64::from(extends_selected),
        gate.live_next_turn_count() + u64::from(!extends_selected),
        gate.live_logical_utf8_bytes() + logical_bytes,
    )
    .unwrap();
    let next_thread_revision = thread.revision().checked_next().unwrap();
    let next_thread = ThreadRecord::new(
        thread_id,
        SelectedPathProof::new(
            thread.committed_tail(),
            next_thread_revision,
            thread.selected_path_digest(),
        ),
        next_draft_id,
        thread.lineage(),
        thread.context_owner_id(),
    );
    let mut staging_thread_bytes = *next_draft_id.as_bytes();
    for byte in &mut staging_thread_bytes {
        *byte ^= 0xe7;
    }
    let root_history = crate::support::seed_detached_canonical_draft_backing(
        store,
        storage.clone(),
        SyndicThreadId::from_bytes(staging_thread_bytes),
        next_draft_id,
    );
    let next_draft_revision = DraftRevision::new(1).unwrap();
    let input = AcceptedInputRecord::new(
        current.draft().id().accepted_input_id(),
        thread_id,
        ordinal,
        AcceptedInputAdmissionProof::new(
            thread.revision(),
            current.draft().id(),
            current.draft().revision(),
            gate.revision(),
            next_draft_id,
        )
        .unwrap(),
        generation_id,
        content,
        None,
        timestamp(at),
    )
    .unwrap();
    let mut fixture = FixtureBatch::new();
    fixture
        .delete(FixtureDelete::Draft(current.draft().id()))
        .unwrap();
    for record in [
        FixtureRecord::Thread(next_thread),
        FixtureRecord::Draft(DraftRecord::new(
            next_draft_id,
            thread_id,
            next_draft_revision,
            DraftSubmissionIntent::Ordinary,
            root_history,
            timestamp(at),
            timestamp(at),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread_id,
            next_draft_id,
            next_draft_revision,
            next_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread_id,
            summary.revision().checked_next().unwrap(),
            next_thread_revision,
            summary.committed_tail(),
            summary.selected_path_digest(),
            summary.complete(),
            timestamp(at),
        )),
        FixtureRecord::InputGate(next_gate),
        FixtureRecord::AcceptedInput(input.clone()),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread_id,
            ordinal,
            input.id(),
            generation_id,
        )),
        FixtureRecord::AcceptedRouteGeneration(route),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input.id(),
            thread_id,
            generation_id,
            ordinal,
            AcceptedInputRevision::new(1).unwrap(),
            next_reason.map_or(
                AcceptedRouteLeafState::Routed,
                AcceptedRouteLeafState::NextTurn,
            ),
            AcceptedInputLifecycle::Admitted,
        )),
    ] {
        fixture.put(record).unwrap();
    }
    if extends_selected {
        fixture
            .put(FixtureRecord::AcceptedRouteGenerationHead(
                AcceptedRouteGenerationHeadRecord::new(thread_id, route_proof),
            ))
            .unwrap();
        fixture
            .put(FixtureRecord::AcceptedReadySource(
                AcceptedReadySourceRecord::new(
                    thread_id,
                    gate_revision,
                    generation_id,
                    generation_revision,
                    first_ordinal.unwrap(),
                    ordinal,
                ),
            ))
            .unwrap();
    } else {
        fixture
            .put(FixtureRecord::AcceptedNextSource(
                AcceptedNextSourceRecord::new(
                    thread_id,
                    generation_id,
                    generation_revision,
                    first_ordinal.unwrap(),
                    ordinal,
                ),
            ))
            .unwrap();
    }
    crate::support::commit(store, storage.clone(), fixture);
    input
}
