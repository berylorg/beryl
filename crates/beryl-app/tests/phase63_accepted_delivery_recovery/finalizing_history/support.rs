use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use beryl_app::{
    cas_projection::{
        ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
        ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
        ScheduledOrdinaryExecutionUnavailable,
    },
    input_admission::prepare_accepted_input_admission,
};
use beryl_model::{RuntimeId, SyndicAcceptedInputId, SyndicDraftId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AcceptedInputAdmission, AcceptedRouteEffectiveState, CasTurnSource, ComposerAtom,
    ComposerPayload, ContentAppend, ContentBuild, DeliveryRecoveryCase, DraftPayloadUpdate,
    DraftPayloadUpdateDecision, InputGateState, LiveSourceEvent, NextTurnReason, PreparedContent,
    SourceEventPayload, SourceEventSequence, SyndicStorage, TurnEndStatus, TurnIncompleteReason,
    TurnLifecycle,
};

use crate::{
    app_support::{execute, point_limit, startup_source, time},
    phase62_support::{CheckoutProvider, NextRecordIds, SUBMITTED_TEXT, accepted_route_state},
};

pub(super) const COMPLETED_PARENT_TEXT: &str = "phase63 finalizing-history completed parent";
pub(super) const STEERING_TEXT: &str = "phase63 single-owner steering";

pub(super) struct FinalizingFixture {
    pub(super) directory: tempfile::TempDir,
    pub(super) thread: SyndicThreadId,
    pub(super) predecessor: SyndicTurnId,
    pub(super) successor: Option<NextRecordIds>,
    pub(super) runtime_id: RuntimeId,
    finalized_before: u64,
    transcript_entries_before: u64,
}

pub(super) struct CountingUnavailableProvider {
    pub(super) attempts: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for CountingUnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

pub(super) struct CountingCheckoutProvider {
    pub(super) attempts: Arc<AtomicUsize>,
    pub(super) checkout: CheckoutProvider,
}

impl ScheduledOrdinaryExecutionProvider for CountingCheckoutProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.checkout.try_issue(admission)
    }

    fn shutdown(&mut self) {
        self.checkout.shutdown();
    }
}

pub(super) fn finalizing_fixture(seed: u8, queue_successor: bool) -> FinalizingFixture {
    let mut fixture = crate::syndic::Fixture::new(seed);
    let completed = fixture.submit_text(COMPLETED_PARENT_TEXT);
    let completed_source = fixture.activate_without_terminal(completed);
    fixture.complete_active_without_assistant(completed, &completed_source);
    let predecessor = fixture.submit_text(SUBMITTED_TEXT);
    fixture.activate_without_terminal(predecessor);

    let command_home = fixture.store.live_home_command().unwrap();
    let home = command_home.home();
    let source = startup_source(home, fixture.storage);
    let DeliveryRecoveryCase::Active(active) = fixture
        .storage
        .classify_delivery_recovery(home, &source, point_limit())
        .unwrap()
    else {
        panic!("finalizing-history fixture must begin with active authority")
    };
    let observed_at = time(63_600 + u64::from(seed));
    execute(
        home,
        fixture.storage.abandon_active_binding(
            fixture.storage.revision(home).unwrap(),
            active
                .generic_abandonment("phase63 durable finalizing-history cut", observed_at)
                .unwrap(),
        ),
    );
    publish_source_less_terminal(
        home,
        fixture.storage,
        fixture.thread,
        predecessor.turn,
        observed_at,
    );

    let state = fixture
        .storage
        .turn_state(home, predecessor.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(predecessor.turn)
    );
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        state.end_status().unwrap().incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
    assert!(
        state.finalized_item_count() < state.item_count(),
        "the restart cut must retain captured history work"
    );
    let head = fixture
        .storage
        .transcript_view_head(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();

    let successor = queue_successor
        .then(|| admit_successor(home, fixture.storage, fixture.thread, predecessor.turn));
    let runtime_id = crate::syndic::execution_binding().runtime_id();
    let cut_source = startup_source(home, fixture.storage);
    assert!(matches!(
        fixture.storage.classify_delivery_recovery(
            home,
            &cut_source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            ..
        }) if thread_id == fixture.thread && turn_id == predecessor.turn
    ));
    home.validate_registered_domains().unwrap();
    drop(command_home);
    let thread = fixture.thread;
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    FinalizingFixture {
        directory,
        thread,
        predecessor: predecessor.turn,
        successor,
        runtime_id,
        finalized_before: state.finalized_item_count(),
        transcript_entries_before: head.entry_count(),
    }
}

fn publish_source_less_terminal(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    observed_at: syndic_storage::SyndicTimestamp,
) {
    publish_live_event(
        store,
        storage,
        thread,
        turn,
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::AuthorityLost,
        )),
        observed_at,
    );
}

fn publish_live_event(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: Option<CasTurnSource>,
    payload: SourceEventPayload,
    observed_at: syndic_storage::SyndicTimestamp,
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
    );
}

pub(super) fn admit_successor(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    predecessor: SyndicTurnId,
) -> NextRecordIds {
    let content = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(SUBMITTED_TEXT).unwrap()]).unwrap(),
    )
    .unwrap();
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let predecessor_state = storage
        .turn_state(store, predecessor, point_limit())
        .unwrap()
        .unwrap();
    let updated_at = time(
        current
            .draft()
            .updated_at()
            .max(predecessor_state.updated_at())
            .unix_millis()
            .checked_add(1)
            .unwrap(),
    );
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &content, updated_at).unwrap()
    else {
        panic!("finalizing-history successor must replace the retained draft")
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
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(predecessor)
    );
    let admission = AcceptedInputAdmission::new(
        thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([250; 16]),
        None,
        time(
            current
                .draft()
                .updated_at()
                .unix_millis()
                .checked_add(1)
                .unwrap(),
        ),
    );
    let accepted_input = admission.accepted_input_id();
    execute(
        store,
        storage.admit_accepted_input(storage.revision(store).unwrap(), admission),
    );
    let successor = NextRecordIds {
        thread,
        accepted_input,
        parent: predecessor,
    };
    assert_eq!(
        accepted_route_state(store, storage, &successor),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
    );
    successor
}

pub(super) fn prepare_steering_draft(fixture: &crate::syndic::Fixture) {
    let prepared = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(STEERING_TEXT).unwrap()]).unwrap(),
    )
    .unwrap();
    let command_home = fixture.store.live_home_command().unwrap();
    let home = command_home.home();
    stage_prepared_content(home, fixture.storage, &prepared);
    let current = fixture
        .storage
        .current_draft(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, time(65_001)).unwrap()
    else {
        panic!("single-owner steering fixture must change the draft")
    };
    execute(
        home,
        fixture
            .storage
            .update_draft_payload(fixture.storage.revision(home).unwrap(), update),
    );
}

pub(super) fn admit_and_wait_for_steering(
    fixture: &crate::syndic::Fixture,
    turn: SyndicTurnId,
) -> SyndicAcceptedInputId {
    crate::phase62_support::wait_until("single-owner turn becomes steerable", || {
        let command_home = fixture.store.live_home_command().ok()?;
        let home = command_home.home();
        let gate = fixture
            .storage
            .input_gate(home, fixture.thread, point_limit())
            .ok()
            .flatten()?;
        let state = fixture
            .storage
            .turn_state(home, turn, point_limit())
            .ok()
            .flatten()?;
        (gate.state() == &InputGateState::Steerable(turn) && state.source_event_count() >= 3)
            .then_some(())
    });
    let (accepted_input, prepared) = {
        let command_home = fixture.store.live_home_command().unwrap();
        let home = command_home.home();
        let current = fixture
            .storage
            .current_draft(home, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = fixture
            .storage
            .input_gate(home, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let state = fixture
            .storage
            .turn_state(home, turn, point_limit())
            .unwrap()
            .unwrap();
        let admission = AcceptedInputAdmission::new(
            fixture.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([249; 16]),
            None,
            state.updated_at().max(current.draft().updated_at()),
        );
        let accepted_input = admission.accepted_input_id();
        let prepared = prepare_accepted_input_admission(
            home,
            fixture.storage,
            fixture.state.assets(),
            admission,
        )
        .unwrap();
        (accepted_input, prepared)
    };
    fixture
        .store
        .execute_accepted_input_admission(prepared)
        .unwrap();
    crate::phase62_support::wait_until("single-owner steering delivery claim", || {
        let command_home = fixture.store.live_home_command().ok()?;
        fixture
            .storage
            .delivering_steering_input(command_home.home(), accepted_input, point_limit())
            .ok()
            .flatten()
            .map(|_| ())
    });
    accepted_input
}

fn stage_prepared_content(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        manifest = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
}

pub(super) fn assert_recovered_predecessor(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    fixture: &FinalizingFixture,
) {
    let gate = storage
        .input_gate(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    let state = storage
        .turn_state(store, fixture.predecessor, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        state.end_status().unwrap().incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
    assert_eq!(state.finalized_item_count(), state.item_count());
    assert!(state.finalized_item_count() > fixture.finalized_before);
    assert_eq!(state.open_item_count(), 0);
    assert_eq!(state.history_blocking_item_count(), 0);
    let head = storage
        .transcript_view_head(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.committed_tail(), Some(fixture.predecessor));
    assert!(head.entry_count() > fixture.transcript_entries_before);
}
