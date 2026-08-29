use beryl_home_store::{CommandError, CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, SyndicAcceptedInputId, SyndicDraftId, SyndicItemId,
};
use syndic_storage::test_faults::{FixtureDelete, FixtureRecord};
use syndic_storage::{
    AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedOrderIndexRecord, AcceptedRouteGeneration, AcceptedRouteGenerationRecord,
    AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteRevision, AcceptedRouteTarget,
    AdvanceItemProjectionBuild, AdvanceTranscriptBuild, CompleteTerminalHistory, ComposerAtom,
    ComposerPayload, DraftByThreadRecord, DraftRecord, DraftSubmissionIntent, FinalizeNextTurnItem,
    FreezeNextTurnItem, HistorySummaryRecord, InputGateRecord, InputGateState,
    ItemProjectionGeneration, NextTurnReason, ProjectionLifecycle, SourceEventPayload,
    StartItemProjectionBuild, StartTranscriptBuild, SyndicMutationError, TranscriptBuildPhase,
    TurnEndStatus, TurnIncompleteReason,
};

use crate::{
    recovery_support::{execute, ordered_id, pending_home, point_limit},
    support::{
        batch, commit, composer_content_records, exact_cas, seed_detached_canonical_draft_backing,
        timestamp,
    },
};

pub(super) fn execute_result(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

pub(super) fn typed_error(outcome: &CommandOutcome) -> &SyndicMutationError {
    let CommandOutcome::NotCommitted { evidence: error } = outcome else {
        panic!("expected not-committed Syndic validation outcome, got {outcome:?}");
    };
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

pub(super) fn terminal_home(
    name: &str,
    value: u64,
    correlate_user: bool,
) -> super::recovery_support::RecoveryHome {
    let recovery = pending_home(name, value);
    let source = exact_cas::establish_turn(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        recovery.turn,
        timestamp(5),
    );
    exact_cas::admit_event(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        recovery.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(6),
    );
    if correlate_user {
        exact_cas::correlate_user_item(
            &recovery.store,
            recovery.storage.clone(),
            recovery.thread,
            recovery.turn,
            SyndicItemId::from_bytes(*ordered_id(value + 30_000).as_bytes()),
            &source,
            timestamp(7),
        );
    }
    let status = if correlate_user {
        TurnEndStatus::complete()
    } else {
        TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost)
    };
    exact_cas::admit_event(
        &recovery.store,
        recovery.storage.clone(),
        recovery.thread,
        recovery.turn,
        &source,
        SourceEventPayload::TurnEnded(status),
        timestamp(8),
    );
    assert_eq!(
        recovery
            .storage
            .input_gate(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(recovery.turn)
    );
    recovery
}

pub(super) fn completion_request(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
) -> CompleteTerminalHistory {
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    CompleteTerminalHistory::new(
        thread,
        turn,
        gate,
        state.revision(),
        head.generation(),
        head.revision(),
    )
}

pub(super) fn finish_transcript(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
) {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ),
    );
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ),
        );
    }
    panic!("bounded finalizing-history transcript build did not finish");
}

pub(super) fn converge_first_item(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    item_id: SyndicItemId,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.freeze_next_turn_item(
            storage.revision(store).unwrap(),
            FreezeNextTurnItem::new(
                thread,
                turn,
                state.revision(),
                syndic_storage::TurnItemOrdinal::FIRST,
                item_id,
                timestamp(9),
            ),
        ),
    );

    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .item_projection_head(store, item_id, point_limit())
        .unwrap();
    let generation = match head.as_ref() {
        Some(head) if head.lifecycle() == ProjectionLifecycle::Current => head.generation(),
        Some(head) => head.generation().checked_next().unwrap(),
        None => ItemProjectionGeneration::FIRST,
    };
    if !head
        .as_ref()
        .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
    {
        execute(
            store,
            storage.start_item_projection_build(
                storage.revision(store).unwrap(),
                StartItemProjectionBuild::new(item_id, item.revision(), generation),
            ),
        );
        for _ in 0..1_024 {
            if storage
                .item_projection_head(store, item_id, point_limit())
                .unwrap()
                .as_ref()
                .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
            {
                break;
            }
            let build = storage
                .item_projection_build(store, item_id, generation, point_limit())
                .unwrap()
                .unwrap();
            execute(
                store,
                storage.advance_item_projection_build(
                    storage.revision(store).unwrap(),
                    AdvanceItemProjectionBuild::new(item_id, generation, build.revision()),
                ),
            );
        }
    }
    assert_eq!(
        storage
            .item_projection_head(store, item_id, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ProjectionLifecycle::Current
    );

    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                turn,
                state.revision(),
                syndic_storage::TurnItemOrdinal::FIRST,
                item_id,
                timestamp(10),
            ),
        ),
    );
}

pub(super) fn queue_input(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    value: u64,
    admitted_at: syndic_storage::SyndicTimestamp,
) -> SyndicAcceptedInputId {
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text(format!("queued while finalizing {value}")).unwrap(),
    ])
    .unwrap();
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let InputGateState::FinalizingHistory(turn) = gate.state() else {
        panic!("queued finalizing-history fixture requires a finalizing gate");
    };
    let history = storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let input_id = current.draft().id().accepted_input_id();
    let replacement = SyndicDraftId::from_bytes(*ordered_id(value + 40_000).as_bytes());
    let replacement_roots = seed_detached_canonical_draft_backing(
        store,
        storage.clone(),
        ordered_id(value + 50_000),
        replacement,
    );
    let ordinal =
        AcceptedInputOrdinal::new(gate.accepted_high_water().checked_add(1).unwrap()).unwrap();
    let generation = gate
        .route_generation_high_water()
        .map(AcceptedRouteGeneration::checked_next)
        .transpose()
        .unwrap()
        .unwrap_or(AcceptedRouteGeneration::FIRST);
    let thread_revision = current.thread().revision().checked_next().unwrap();
    let gate_revision = gate.revision().checked_next().unwrap();
    let (content, mut records) = composer_content_records(&payload);
    let content_bytes = content.summary().logical_utf8_bytes();
    records.extend([
        FixtureRecord::Thread(syndic_storage::ThreadRecord::new(
            thread,
            syndic_storage::SelectedPathProof::new(
                current.thread().committed_tail(),
                thread_revision,
                current.thread().selected_path_digest(),
            ),
            replacement,
            current.thread().lineage(),
            current.thread().context_owner_id(),
        )),
        FixtureRecord::Draft(DraftRecord::new(
            replacement,
            thread,
            DraftRevision::new(1).unwrap(),
            DraftSubmissionIntent::Ordinary,
            replacement_roots,
            admitted_at,
            admitted_at,
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            replacement,
            DraftRevision::new(1).unwrap(),
            thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            history.revision().checked_next().unwrap(),
            thread_revision,
            history.committed_tail(),
            history.selected_path_digest(),
            history.complete(),
            admitted_at,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                gate_revision,
                InputGateState::FinalizingHistory(*turn),
                ordinal.get(),
                Some(generation),
                gate.selected_route(),
                0,
                gate.live_next_turn_count().checked_add(1).unwrap(),
                gate.live_logical_utf8_bytes()
                    .checked_add(content_bytes)
                    .unwrap(),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input_id,
                thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    current.thread().revision(),
                    current.draft().id(),
                    current.draft().revision(),
                    gate.revision(),
                    replacement,
                )
                .unwrap(),
                generation,
                content,
                None,
                admitted_at,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread, ordinal, input_id, generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
                AcceptedRouteTarget::NextTurn(NextTurnReason::TerminalHistory),
                Some(ordinal),
                Some(ordinal),
                1,
                0,
                0,
                1,
                0,
                content_bytes,
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input_id,
            thread,
            generation,
            ordinal,
            AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::NextTurn(NextTurnReason::TerminalHistory),
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedNextSource(syndic_storage::AcceptedNextSourceRecord::new(
            thread,
            generation,
            AcceptedRouteRevision::FIRST,
            ordinal,
            ordinal,
        )),
    ]);
    let mut fixture = batch(records);
    fixture
        .delete(FixtureDelete::Draft(current.draft().id()))
        .unwrap();
    commit(store, storage, fixture);
    input_id
}
