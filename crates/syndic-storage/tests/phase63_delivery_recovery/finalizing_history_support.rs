use beryl_home_store::{CommandError, HomeCommand, HomeStore};
use beryl_model::{SyndicDraftId, SyndicItemId};
use syndic_storage::{
    AcceptedInputAdmission, AdvanceItemProjectionBuild, AdvanceTranscriptBuild,
    CompleteTerminalHistory, ComposerAtom, ComposerPayload, DraftPayloadUpdate,
    DraftPayloadUpdateDecision, FinalizeNextTurnItem, FreezeNextTurnItem, InputGateState,
    ItemProjectionGeneration, PreparedContent, ProjectionLifecycle, SourceEventPayload,
    StartItemProjectionBuild, StartTranscriptBuild, SyndicMutationError, TranscriptBuildPhase,
    TurnEndStatus, TurnIncompleteReason,
};

use crate::{
    recovery_support::{execute, ordered_id, pending_home, point_limit},
    support::{exact_cas, stage_prepared_content, timestamp},
};

pub(super) fn execute_result(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).map(|_| ())
}

pub(super) fn typed_error(error: &CommandError) -> &SyndicMutationError {
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
        recovery.storage,
        recovery.thread,
        recovery.turn,
        timestamp(5),
    );
    exact_cas::admit_event(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(6),
    );
    if correlate_user {
        exact_cas::correlate_user_item(
            &recovery.store,
            recovery.storage,
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
        recovery.storage,
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
    updated_at: syndic_storage::SyndicTimestamp,
    admitted_at: syndic_storage::SyndicTimestamp,
) -> AcceptedInputAdmission {
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text(format!("queued while finalizing {value}")).unwrap(),
    ])
    .unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &prepared);
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, updated_at).unwrap()
    else {
        panic!("queued finalizing-history draft must become nonempty");
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes(*ordered_id(value + 40_000).as_bytes()),
        None,
        admitted_at,
    );
    execute(
        store,
        storage.admit_accepted_input(storage.revision(store).unwrap(), admission.clone()),
    );
    admission
}
