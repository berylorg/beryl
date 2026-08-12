use beryl_app::cas_projection::{
    AdmittedProjectionSession, LiveEventConnectionState, LoadedCasProjection,
    test_faults::{last_websocket_ingress_snapshot, provider_broker_snapshot},
};
use beryl_home_store::HomeGeneration;
use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasNativeTurnCount, CasThreadId,
    ExecutionBinding, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    BindingState, CasItemSource, CasLineageProof, CasTurnSource, InputGateState,
    ProviderFrameObservationSummaryV1, ProviderFrameOrdinalV1, ProviderItemLifecycle,
    ProviderLifecycleTimestampMsV1, SourceEventPayload, SourceEventSequence, TurnEndStatus,
    TurnLifecycle,
};

use super::{
    server::{CAS_ITEM_ID, CAS_THREAD_ID, CAS_TURN_ID, COMPLETED_AT_MS, STARTED_AT_MS},
    syndic::{Fixture, point_limit},
};

pub struct ProjectionExpectation {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    syndic_thread_id: SyndicThreadId,
    initial_binding_revision: BindingRevision,
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    lineage: CasLineageProof,
}

impl ProjectionExpectation {
    pub fn capture(projection: &LoadedCasProjection) -> Self {
        assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD_ID);
        Self {
            home_id: projection.home_id(),
            home_generation: projection.home_generation(),
            syndic_thread_id: projection.syndic_thread_id(),
            initial_binding_revision: projection.binding_revision(),
            execution: projection.execution_binding().clone(),
            cas_thread_id: projection.cas_thread_id().clone(),
            loaded_generation: projection.loaded_session_generation(),
            lineage: projection.lineage_proof(),
        }
    }

    pub fn assert_returned(
        &self,
        projection: &LoadedCasProjection,
        binding_revision: BindingRevision,
    ) {
        assert_eq!(projection.home_id(), self.home_id);
        assert_eq!(projection.home_generation(), self.home_generation);
        assert_eq!(projection.syndic_thread_id(), self.syndic_thread_id);
        assert_eq!(projection.binding_revision(), binding_revision);
        assert_eq!(
            binding_revision.get(),
            self.initial_binding_revision.get().checked_add(2).unwrap(),
        );
        assert_eq!(projection.execution_binding(), &self.execution);
        assert_eq!(projection.cas_thread_id(), &self.cas_thread_id);
        assert_eq!(
            projection.loaded_session_generation(),
            self.loaded_generation
        );
        assert_eq!(projection.lineage_proof(), self.lineage);
        assert!(projection.is_live().unwrap());
    }
}

pub struct DurableSuccess {
    pub binding_revision: BindingRevision,
}

pub fn assert_durable_success(
    fixture: &Fixture,
    turn: SyndicTurnId,
    expected_status: TurnEndStatus,
) -> DurableSuccess {
    let source = CasTurnSource::new(
        CasThreadId::new(CAS_THREAD_ID).unwrap(),
        beryl_model::CasTurnId::new(CAS_TURN_ID).unwrap(),
    );
    let state = fixture
        .storage
        .turn_state(&*fixture.home(), turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.source_event_count(), 4);
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.finalized_item_count(), 1);
    assert_eq!(state.open_item_count(), 0);
    assert_eq!(state.history_blocking_item_count(), 0);
    assert_eq!(state.provider_observation_issue(), None);
    assert_eq!(state.end_status(), Some(expected_status));
    assert_eq!(state.incomplete_reason(), None);

    let items = fixture
        .storage
        .turn_items(
            &*fixture.home(),
            turn,
            None,
            beryl_home_store::CursorReadLimits::new(2, 64 * 1024).unwrap(),
        )
        .unwrap();
    assert!(!items.has_more());
    assert_eq!(items.records().len(), 1);
    let item_id = items.records()[0].item_id();
    let cas_item_id = beryl_model::CasItemId::new(CAS_ITEM_ID).unwrap();

    let activation = source_event(fixture, turn, 1);
    assert_eq!(activation.source(), Some(&source));
    assert!(matches!(
        activation.payload(),
        SourceEventPayload::TurnActivated
    ));
    assert_checked_frame(
        fixture,
        turn,
        2,
        &source,
        item_id,
        &cas_item_id,
        ProviderFrameOrdinalV1::FIRST,
        ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(
            STARTED_AT_MS,
        )),
    );
    assert_checked_frame(
        fixture,
        turn,
        3,
        &source,
        item_id,
        &cas_item_id,
        ProviderFrameOrdinalV1::new(2).unwrap(),
        ProviderFrameObservationSummaryV1::Completed(ProviderLifecycleTimestampMsV1::new(
            COMPLETED_AT_MS,
        )),
    );
    let terminal = source_event(fixture, turn, 4);
    assert_eq!(terminal.source(), Some(&source));
    assert!(matches!(
        terminal.payload(),
        SourceEventPayload::TurnEnded(status) if *status == expected_status
    ));

    let item = fixture
        .storage
        .canonical_item(&*fixture.home(), item_id, point_limit())
        .unwrap()
        .unwrap();
    let item_source = CasItemSource::new(source.clone(), cas_item_id);
    assert_eq!(item.cas_source(), Some(&item_source));
    assert_eq!(
        item.source_event(),
        Some(SourceEventSequence::new(3).unwrap())
    );
    assert_eq!(item.source_event_count(), 2);
    assert_eq!(item.provider_lifecycle(), ProviderItemLifecycle::Completed);
    assert!(item.provider().unwrap().stream_state().is_complete());
    let captured = fixture
        .storage
        .capture_item(&*fixture.home(), &item_source, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(captured.item(), &item);

    let gate = fixture
        .storage
        .input_gate(&*fixture.home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_count(), 0);

    let binding = fixture
        .storage
        .current_binding(&*fixture.home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("normal terminal did not restore a valid binding")
    };
    let selected = fixture.selected_path(fixture.thread);
    let represented = usable.represented_prefix();
    assert_eq!(represented.tail(), Some(turn));
    assert_eq!(
        represented.source_thread_revision(),
        selected.thread_revision()
    );
    assert_eq!(represented.digest(), selected.digest());
    assert_eq!(usable.native_turn_count(), CasNativeTurnCount::new(1));
    assert_eq!(usable.cas_thread_id().as_str(), CAS_THREAD_ID);
    assert_eq!(usable.execution(), &super::syndic::execution_binding());

    DurableSuccess {
        binding_revision: binding.binding().revision(),
    }
}

fn source_event(
    fixture: &Fixture,
    turn: SyndicTurnId,
    sequence: u64,
) -> syndic_storage::SourceEventRecord {
    fixture
        .storage
        .source_event(
            &*fixture.home(),
            turn,
            SourceEventSequence::new(sequence).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn assert_checked_frame(
    fixture: &Fixture,
    turn: SyndicTurnId,
    sequence: u64,
    source: &CasTurnSource,
    item_id: beryl_model::SyndicItemId,
    cas_item_id: &beryl_model::CasItemId,
    ordinal: ProviderFrameOrdinalV1,
    observation: ProviderFrameObservationSummaryV1,
) {
    let event = source_event(fixture, turn, sequence);
    assert_eq!(event.source(), Some(source));
    let SourceEventPayload::ItemFrame {
        item_id: actual_item,
        frame,
    } = event.payload()
    else {
        panic!("source event {sequence} was not a checked user frame")
    };
    assert_eq!(*actual_item, item_id);
    assert_eq!(frame.frame().item_id(), cas_item_id);
    assert_eq!(frame.frame().ordinal(), ordinal);
    assert_eq!(frame.observation(), observation);
}

pub fn assert_connection_quiescent(
    session: &AdmittedProjectionSession,
    terminal_wire_bytes: usize,
) {
    let broker = provider_broker_snapshot(session);
    assert_eq!(broker.in_flight().current(), 0);
    assert_eq!(broker.in_flight().high_water(), 1);
    assert_eq!(broker.submitted(), 3);
    assert_eq!(broker.acked(), 3);
    assert_eq!(broker.staged_fragments().current(), 0);
    let checked = broker.checked_user_publications();
    assert_eq!(checked.activity().current(), 0);
    assert_eq!(checked.activity().high_water(), 1);
    assert_eq!(checked.publications(), 2);

    let pages = session.provider_page_diagnostics();
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.available, pages.page_count);
    let router = session.live_event_snapshot().unwrap();
    assert_eq!(router.state(), LiveEventConnectionState::Active);
    assert_eq!(router.target_count(), 0);
    assert_eq!(router.queued_operation_count(), 0);
    assert_eq!(router.outstanding_dynamic_tool_count(), 0);
    assert_eq!(router.routed_operation_count(), 3);
    assert_eq!(router.unmatched_operation_count(), 0);
    assert_eq!(router.rejected_operation_count(), 0);
    assert_eq!(router.queue_pressure_count(), 0);

    let process = session.live_event_process_snapshot().unwrap();
    assert_eq!(process.active_connection_count(), 1);
    let ingress = last_websocket_ingress_snapshot(session)
        .unwrap()
        .expect("terminal decode records ingress diagnostics");
    assert_eq!(ingress.message_bytes(), terminal_wire_bytes);
    assert!((1..=8 * 1024).contains(&ingress.maximum_transport_chunk_bytes()));
    assert!((1..=8 * 1024).contains(&ingress.maximum_parser_buffer_bytes()));
    assert_eq!(ingress.discarded_image_result_bytes(), 0);
    assert!(!ingress.retained_item_result_present());
}
