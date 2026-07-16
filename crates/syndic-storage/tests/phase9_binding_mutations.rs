#![cfg(feature = "test-faults")]

mod support;

#[path = "phase9_binding_mutations/abandonment.rs"]
mod abandonment;
#[path = "phase9_binding_mutations/lifecycle.rs"]
mod lifecycle;
#[path = "phase9_binding_mutations/local_terminal.rs"]
mod local_terminal;

use beryl_home_store::{CommandError, DomainRegistrationError, HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    CasThreadId, CasTurnId, ExecutionBinding, InputGateRevision, PathFlavor, RootId, RuntimeId,
    RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::*;

use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn execute_result(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).map(|_| ())
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic mutation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn create_thread(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
) {
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(thread, draft, timestamp(1)),
        ),
    );
}

fn save_text(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    text: &str,
    updated_at: SyndicTimestamp,
) {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, updated_at).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("test payload must change"),
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
}

fn submit_root_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    replacement: SyndicDraftId,
    item: SyndicItemId,
    submitted_at: SyndicTimestamp,
) -> (SyndicTurnId, SelectedPathProof) {
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        current.thread().revision(),
        draft,
        current.draft().revision(),
        current.draft().content(),
        gate.record().revision(),
        replacement,
        item,
        AdmissionMarkers::default(),
        submitted_at,
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    (
        turn,
        SelectedPathProof::new(
            Some(turn),
            current.thread().revision(),
            current.thread().selected_path_digest(),
        ),
    )
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([90; 16]),
        RootId::from_bytes([91; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase9-binding",
        )
        .unwrap(),
    )
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(7).unwrap(),
        CasLoadedThreadGeneration::new(11).unwrap(),
    )
}

fn valid_request(
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
) -> PublishValidBinding {
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    PublishValidBinding::new(
        thread,
        BindingRevision::new(2).unwrap(),
        selected,
        execution_binding(),
        cas_thread,
        represented,
        beryl_model::CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    )
}

struct ActiveTurnFixture {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    snapshot: SyndicExecutionSnapshotId,
    cas_thread: CasThreadId,
    cas_turn: CasTurnId,
    valid: PublishValidBinding,
    activation: ActivateBinding,
}

fn activate_root_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    publish_cas_turn: bool,
) -> ActiveTurnFixture {
    let thread = id(20);
    let draft = draft_id(21);
    let replacement = draft_id(22);
    create_thread(store, storage, thread, draft);
    save_text(store, storage, thread, "active root", timestamp(2));
    let (turn, selected) = submit_root_turn(
        store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([23; 16]),
        timestamp(3),
    );
    let cas_thread = CasThreadId::new("phase9-terminal-authority").unwrap();
    let valid = valid_request(thread, selected, cas_thread.clone());
    execute(
        store,
        storage.publish_valid_binding(storage.revision(store).unwrap(), valid.clone()),
    );
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let source_less_activation = LiveSourceEvent::new(
        thread,
        turn,
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::FIRST,
        None,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    )
    .unwrap();
    let error = execute_result(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), source_less_activation),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));
    let snapshot = SyndicExecutionSnapshotId::from_bytes([24; 16]);
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let activation = ActivateBinding::new(
        thread,
        current.binding().revision(),
        gate.record().revision(),
        selected,
        snapshot,
        turn,
        loaded_generation(),
        timestamp(4),
    );
    execute(
        store,
        storage.activate_binding(storage.revision(store).unwrap(), activation.clone()),
    );
    let cas_turn = CasTurnId::new("phase9-terminal-cas-turn").unwrap();
    if publish_cas_turn {
        let binding = storage
            .current_binding(store, thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(store, thread, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.publish_active_cas_turn(
                storage.revision(store).unwrap(),
                PublishActiveCasTurn::new(
                    thread,
                    binding.binding().revision(),
                    gate.record().revision(),
                    snapshot,
                    cas_thread.clone(),
                    cas_turn.clone(),
                    timestamp(5),
                ),
            ),
        );
    }
    ActiveTurnFixture {
        thread,
        turn,
        selected,
        snapshot,
        cas_thread,
        cas_turn,
        valid,
        activation,
    }
}

fn terminal_event(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &ActiveTurnFixture,
    source: Option<CasTurnSource>,
    outcome: TurnTerminalOutcome,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let state = storage
        .turn_state(store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::new(state.record().source_event_count() + 1).unwrap(),
        source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(outcome, Some(TurnIncompleteReason::ItemAuditFailed)).unwrap(),
        ),
        observed_at,
    )
    .unwrap()
}
