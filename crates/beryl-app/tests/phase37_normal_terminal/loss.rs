use std::path::Path;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
    LiveEventConnectionState, LiveEventTargetCloseReason, LoadedCasProjection,
    OrdinaryDynamicToolHandlers, OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, test_faults::provider_broker_snapshot,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_home_store::CursorReadLimits;
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, ExecutionBinding, SyndicTurnId,
};
use syndic_storage::{
    BindingState, CasItemSource, CasLineageProof, CasRepresentedPrefixProof, CasTurnSource,
    InputGateState, ProviderFrameObservationSummaryV1, ProviderFrameOrdinalV1,
    ProviderItemLifecycle, ProviderLifecycleTimestampMsV1, SourceEventPayload, SourceEventSequence,
    SyndicTimestamp, TurnEndStatus, TurnIncompleteReason, TurnLifecycle,
};

use super::{
    EXECUTION_ROOT, NoopBranch, NoopLifecycle,
    server::{
        AUTHORIZATION, CAS_ITEM_ID, CAS_THREAD_ID, CAS_TURN_ID, COMPLETED_AT_MS,
        NormalTerminalServer, STARTED_AT_MS, SUBMITTED_TEXT, TIMEOUT,
    },
    syndic::{Fixture, execution_binding, point_limit},
};

const STALE_REASON: &str = "ordinary turn lost live CAS projection authority";

pub fn run() {
    let mut fixture = Fixture::new(138);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_connection_loss();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_138).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_100),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let expected_projection = LossProjectionExpectation::capture(&fixture, &projection);

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let mut lifecycle = NoopLifecycle::default();
    let mut branch = NoopBranch::default();
    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            fixture.state.assets(),
            projection,
            &fixture.cancellation,
            &execution_request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Incomplete { reason } = outcome else {
        panic!("raw connection loss did not converge as incomplete: {outcome:?}")
    };
    if !matches!(reason, OrdinaryTurnCaptureLoss::TargetClosed(_)) {
        panic!("raw connection loss returned the wrong incomplete category: {reason:?}")
    }
    assert_eq!(lifecycle.calls, 0);
    assert_eq!(branch.calls, 0);

    assert_durable_loss(&fixture, submitted.turn, &expected_projection);
    session.invalidate_connection();
    assert_connection_released(&session);
    drop(session);
    server.join();

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

struct LossProjectionExpectation {
    initial_binding_revision: BindingRevision,
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    represented_prefix: CasRepresentedPrefixProof,
    native_turn_count: CasNativeTurnCount,
    tool_profile: CasConversationToolProfile,
    lineage: CasLineageProof,
}

impl LossProjectionExpectation {
    fn capture(fixture: &Fixture, projection: &LoadedCasProjection) -> Self {
        assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD_ID);
        let binding = fixture
            .storage
            .current_binding(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(binding.binding().revision(), projection.binding_revision());
        let BindingState::Valid(usable) = binding.binding().state() else {
            panic!("fresh raw projection did not retain a valid binding")
        };
        assert_eq!(usable.execution(), projection.execution_binding());
        assert_eq!(usable.cas_thread_id(), projection.cas_thread_id());
        assert_eq!(usable.lineage(), projection.lineage_proof());
        Self {
            initial_binding_revision: projection.binding_revision(),
            execution: projection.execution_binding().clone(),
            cas_thread_id: projection.cas_thread_id().clone(),
            loaded_generation: projection.loaded_session_generation(),
            represented_prefix: usable.represented_prefix(),
            native_turn_count: usable.native_turn_count(),
            tool_profile: usable.tool_profile(),
            lineage: usable.lineage(),
        }
    }
}

fn assert_durable_loss(
    fixture: &Fixture,
    turn: SyndicTurnId,
    expected: &LossProjectionExpectation,
) {
    let status = TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost);
    let state = fixture
        .storage
        .turn_state(&fixture.store, turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(state.source_event_count(), 4);
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.finalized_item_count(), state.item_count());
    assert_eq!(state.open_item_count(), 0);
    assert_eq!(state.history_blocking_item_count(), 0);
    assert_eq!(state.provider_observation_issue(), None);
    assert_eq!(state.end_status(), Some(status));
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::StreamLost)
    );

    let source = CasTurnSource::new(
        CasThreadId::new(CAS_THREAD_ID).unwrap(),
        beryl_model::CasTurnId::new(CAS_TURN_ID).unwrap(),
    );
    let activation = source_event(fixture, turn, 1);
    assert_eq!(activation.source(), Some(&source));
    assert!(matches!(
        activation.payload(),
        SourceEventPayload::TurnActivated
    ));

    let items = fixture
        .storage
        .turn_items(
            &fixture.store,
            turn,
            None,
            CursorReadLimits::new(2, 64 * 1024).unwrap(),
        )
        .unwrap();
    assert!(!items.has_more());
    assert_eq!(items.records().len(), 1);
    let item_id = items.records()[0].item_id();
    let cas_item_id = beryl_model::CasItemId::new(CAS_ITEM_ID).unwrap();
    let started = source_event(fixture, turn, 2);
    assert_eq!(started.source(), Some(&source));
    let SourceEventPayload::ItemFrame {
        item_id: actual_item,
        frame,
    } = started.payload()
    else {
        panic!("source event 2 was not the checked started user frame")
    };
    assert_eq!(*actual_item, item_id);
    assert_eq!(frame.frame().item_id(), &cas_item_id);
    assert_eq!(frame.frame().ordinal(), ProviderFrameOrdinalV1::FIRST);
    assert_eq!(
        frame.observation(),
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(
            STARTED_AT_MS,
        ))
    );

    let completed = source_event(fixture, turn, 3);
    assert_eq!(completed.source(), Some(&source));
    let SourceEventPayload::ItemFrame {
        item_id: actual_item,
        frame,
    } = completed.payload()
    else {
        panic!("source event 3 was not the checked completed user frame")
    };
    assert_eq!(*actual_item, item_id);
    assert_eq!(frame.frame().item_id(), &cas_item_id);
    assert_eq!(
        frame.frame().ordinal(),
        ProviderFrameOrdinalV1::new(2).unwrap()
    );
    assert_eq!(
        frame.observation(),
        ProviderFrameObservationSummaryV1::Completed(ProviderLifecycleTimestampMsV1::new(
            COMPLETED_AT_MS,
        ))
    );

    let terminal = source_event(fixture, turn, 4);
    assert_eq!(terminal.source(), None);
    assert!(matches!(
        terminal.payload(),
        SourceEventPayload::TurnEnded(actual) if *actual == status
    ));
    assert!(
        fixture
            .storage
            .source_event(
                &fixture.store,
                turn,
                SourceEventSequence::new(5).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );

    let item = fixture
        .storage
        .canonical_item(&fixture.store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let item_source = CasItemSource::new(source, cas_item_id);
    assert_eq!(item.cas_source(), Some(&item_source));
    assert_eq!(
        item.source_event(),
        Some(SourceEventSequence::new(3).unwrap())
    );
    assert_eq!(item.source_event_count(), 2);
    assert_eq!(item.provider_lifecycle(), ProviderItemLifecycle::Completed);
    let provider = item.provider().unwrap();
    assert_eq!(
        provider.stream_state().started_at(),
        Some(ProviderLifecycleTimestampMsV1::new(STARTED_AT_MS))
    );
    assert!(provider.stream_state().is_complete());
    let captured = fixture
        .storage
        .capture_item(&fixture.store, &item_source, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(captured.item(), &item);

    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_count(), 0);

    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.binding().revision().get(),
        expected
            .initial_binding_revision
            .get()
            .checked_add(2)
            .unwrap()
    );
    assert_eq!(
        binding.binding().selected_path(),
        fixture.selected_path(fixture.thread)
    );
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("source loss did not abandon the exact active binding")
    };
    assert_eq!(stale.execution(), &expected.execution);
    assert_eq!(stale.cas_thread_id(), &expected.cas_thread_id);
    assert_eq!(stale.observed_tool_profile(), Some(expected.tool_profile));
    assert_eq!(stale.observed_prefix(), Some(expected.represented_prefix));
    assert_eq!(stale.observed_lineage(), Some(expected.lineage));
    assert_eq!(
        stale.observed_native_turn_count(),
        Some(expected.native_turn_count)
    );
    assert_eq!(stale.loaded_generation(), Some(expected.loaded_generation));
    assert_eq!(stale.reason(), STALE_REASON);
}

fn source_event(
    fixture: &Fixture,
    turn: SyndicTurnId,
    sequence: u64,
) -> syndic_storage::SourceEventRecord {
    fixture
        .storage
        .source_event(
            &fixture.store,
            turn,
            SourceEventSequence::new(sequence).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap()
}

fn assert_connection_released(session: &AdmittedProjectionSession) {
    let broker = provider_broker_snapshot(session);
    assert_eq!(broker.in_flight().current(), 0);
    assert_eq!(broker.in_flight().high_water(), 1);
    assert_eq!(broker.submitted(), 2);
    assert_eq!(broker.acked(), 2);
    assert_eq!(broker.staged_fragments().current(), 0);
    assert_eq!(broker.staged_fragment_batches(), 0);
    let checked = broker.checked_user_publications();
    assert_eq!(checked.activity().current(), 0);
    assert_eq!(checked.activity().high_water(), 1);
    assert_eq!(checked.publications(), 2);

    let pages = session.provider_page_diagnostics();
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.available, pages.page_count);
    let router = session.live_event_snapshot().unwrap();
    assert_eq!(
        router.state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::StreamFailure)
    );
    assert_eq!(router.target_count(), 0);
    assert_eq!(router.queued_operation_count(), 0);
    assert_eq!(router.outstanding_dynamic_tool_count(), 0);
    assert_eq!(router.routed_operation_count(), 2);
    assert_eq!(router.unmatched_operation_count(), 0);
    assert_eq!(router.rejected_operation_count(), 0);
    assert_eq!(router.queue_pressure_count(), 0);
    assert_eq!(router.retired_thread_lane_count(), 1);

    let process = session.live_event_process_snapshot().unwrap();
    assert_eq!(process.active_connection_count(), 0);
}
