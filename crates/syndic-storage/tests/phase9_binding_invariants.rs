#![cfg(feature = "test-faults")]

mod support;

#[path = "phase9_binding_invariants/native_turn_count.rs"]
mod native_turn_count;
#[path = "phase9_binding_invariants/recovered_handoff.rs"]
mod recovered_handoff;
#[path = "phase9_binding_invariants/recovered_lineage.rs"]
mod recovered_lineage;
#[path = "phase9_binding_invariants/reopen_binding_records.rs"]
mod reopen_binding_records;
#[path = "phase9_binding_invariants/reopen_correlations.rs"]
mod reopen_correlations;
#[path = "phase9_binding_invariants/retirement.rs"]
mod retirement;
#[path = "phase9_binding_invariants/route_allocator.rs"]
mod route_allocator;
#[path = "phase9_binding_invariants/selected_prefix.rs"]
mod selected_prefix;

use beryl_home_store::{
    CommandError, CursorReadLimits, DomainRegistrationError, HomeCommand, HomeStore,
};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, CasTurnId, ExecutionBinding, InputGateRevision, PathFlavor,
    RecoveryItemSequenceDigest, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::*;

use support::populated::{active_snapshot as populated_active_snapshot, populated_records};
use support::semantic::exercise_case;
use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
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

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([90; 16]),
        RootId::from_bytes([91; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase9-binding-invariants",
        )
        .unwrap(),
    )
}

fn loaded_generation(process: u64, thread: u64) -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(process).unwrap(),
        CasLoadedThreadGeneration::new(thread).unwrap(),
    )
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
            CreateThread::ordinary(thread, draft, execution_binding(), timestamp(1)),
        ),
    )
    .unwrap();
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
    )
    .unwrap();
}

fn submit_current(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
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
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        replacement,
        item,
        None,
        submitted_at,
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    )
    .unwrap();
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

fn admit_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        source,
        payload,
        observed_at,
    )
    .unwrap();
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    )
    .unwrap();
}

fn activate_exact_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> CasTurnSource {
    let selected = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .selected_path();
    let turn_record = storage.turn(store, turn, point_limit()).unwrap().unwrap();
    let (parent, parent_digest) = match turn_record.parent().turn() {
        Some(parent) => {
            let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, empty_selected_path_digest()),
    };
    let represented =
        CasRepresentedPrefixProof::new(parent, selected.thread_revision(), parent_digest);
    let current_revision = current_binding_revision(store, storage, thread);
    let prior = current_revision
        .get()
        .checked_sub(1)
        .and_then(|revision| BindingRevision::new(revision).ok())
        .and_then(|revision| {
            storage
                .binding(store, thread, revision, point_limit())
                .unwrap()
        });
    let (cas_thread, lineage) = match prior.as_ref().map(|stored| stored.state()) {
        Some(BindingState::Valid(usable))
            if usable.represented_prefix().tail() == represented.tail()
                && usable.represented_prefix().digest() == represented.digest() =>
        {
            (usable.cas_thread_id().clone(), usable.lineage())
        }
        _ => {
            let cas_thread = CasThreadId::new(format!("fixture-thread-{turn}")).unwrap();
            let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
            (cas_thread, lineage)
        }
    };
    publish_valid(
        store,
        storage,
        valid_request(
            store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            lineage,
        ),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes());
    let gate = current_gate_revision(store, storage, thread);
    execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(store, storage, thread),
                gate,
                selected,
                snapshot,
                turn,
                loaded_generation(1, 1),
                timestamp(4),
            ),
        ),
    )
    .unwrap();
    let cas_turn = CasTurnId::new(format!("fixture-turn-{turn}")).unwrap();
    execute(
        store,
        storage.publish_active_cas_turn(
            storage.revision(store).unwrap(),
            PublishActiveCasTurn::new(
                thread,
                current_binding_revision(store, storage, thread),
                current_gate_revision(store, storage, thread),
                snapshot,
                cas_thread.clone(),
                cas_turn.clone(),
                timestamp(4),
            ),
        ),
    )
    .unwrap();
    CasTurnSource::new(cas_thread, cas_turn)
}

fn complete_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) {
    let source = activate_exact_turn(store, storage, thread, turn);
    admit_event(
        store,
        storage,
        thread,
        turn,
        Some(source.clone()),
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(store, storage, thread, turn, &source);
    admit_event(
        store,
        storage,
        thread,
        turn,
        Some(source),
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(5),
    );
    converge_and_release_terminal_history(store, storage, thread, turn);
}

fn correlate_submitted_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
) {
    let index = storage
        .turn_items(
            store,
            turn,
            None,
            CursorReadLimits::new(2, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()[0]
        .clone();
    support::exact_cas::correlate_user_item(
        store,
        storage,
        thread,
        turn,
        index.item_id(),
        source,
        timestamp(4),
    );
}

fn root_pending(
    store: &HomeStore,
    storage: SyndicStorage,
) -> (SyndicThreadId, SyndicTurnId, SelectedPathProof) {
    let thread = id(1);
    create_thread(store, storage, thread, draft_id(2));
    save_text(store, storage, thread, "root", timestamp(2));
    let (turn, selected) = submit_current(
        store,
        storage,
        thread,
        draft_id(3),
        SyndicItemId::from_bytes([4; 16]),
        timestamp(3),
    );
    (thread, turn, selected)
}

fn non_root_pending(
    store: &HomeStore,
    storage: SyndicStorage,
) -> (
    SyndicThreadId,
    SyndicTurnId,
    SyndicTurnId,
    SelectedPathProof,
) {
    let (thread, parent, _) = root_pending(store, storage);
    complete_turn(store, storage, thread, parent);
    save_text(store, storage, thread, "child", timestamp(6));
    let (turn, selected) = submit_current(
        store,
        storage,
        thread,
        draft_id(5),
        SyndicItemId::from_bytes([6; 16]),
        timestamp(7),
    );
    (thread, parent, turn, selected)
}

fn current_binding_revision(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> BindingRevision {
    storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .head()
        .revision()
}

fn current_gate_revision(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> InputGateRevision {
    storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .revision()
}

fn valid_request(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
    represented: CasRepresentedPrefixProof,
    lineage: CasLineageProof,
) -> PublishValidBinding {
    valid_request_with_count(
        store,
        storage,
        thread,
        selected,
        cas_thread,
        represented,
        CasNativeTurnCount::ZERO,
        lineage,
    )
}

#[allow(clippy::too_many_arguments)]
fn valid_request_with_count(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    cas_thread: CasThreadId,
    represented: CasRepresentedPrefixProof,
    native_turn_count: CasNativeTurnCount,
    lineage: CasLineageProof,
) -> PublishValidBinding {
    let execution = storage
        .thread_execution(store, thread, point_limit())
        .unwrap()
        .expect("binding fixture thread must retain canonical execution")
        .execution()
        .clone();
    PublishValidBinding::new(
        thread,
        current_binding_revision(store, storage, thread),
        selected,
        execution,
        cas_thread,
        represented,
        native_turn_count,
        test_tool_profile(),
        lineage,
    )
}

fn publish_valid(store: &HomeStore, storage: SyndicStorage, request: PublishValidBinding) {
    execute(
        store,
        storage.publish_valid_binding(storage.revision(store).unwrap(), request),
    )
    .unwrap();
}
