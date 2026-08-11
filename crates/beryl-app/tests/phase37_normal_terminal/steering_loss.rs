use std::{
    fmt::Write as _,
    path::Path,
    time::{Duration, Instant},
};

use beryl_app::{
    cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, OrdinaryDynamicToolHandlers,
        OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    },
    input_admission::prepare_accepted_input_admission,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{CasProcessGeneration, SyndicAcceptedInputId, SyndicDraftId, SyndicTurnId};
use syndic_storage::{
    AcceptedInputAdmission, AcceptedRouteEffectiveState, BindingState, ComposerAtom,
    ComposerPayload, ContentAppend, ContentBuild, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    InputGateState, PreparedContent, SyndicTimestamp, TurnIncompleteReason, TurnLifecycle,
};

use super::{
    EXECUTION_ROOT, NoopBranch, NoopLifecycle,
    server::{AUTHORIZATION, NormalTerminalServer, SteeringFailureTrigger, TIMEOUT},
    syndic::{Fixture, execution_binding, point_limit},
};

pub fn run() {
    let mut fixture = Fixture::new(152);
    let submitted = fixture.submit_text(super::server::SUBMITTED_TEXT);
    prepare_steering_draft(&fixture);
    let server = NormalTerminalServer::spawn_steering_correlation_loss();
    let trigger = server.steering_failure_trigger();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(52_152).unwrap(),
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
        SyndicTimestamp::from_unix_millis(52_000),
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

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let mut lifecycle = NoopLifecycle::default();
    let mut branch = NoopBranch::default();
    let (accepted_input, outcome) = std::thread::scope(|scope| {
        let delivery = scope.spawn(|| admit_claim_and_trigger(&fixture, submitted.turn, trigger));
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
        (delivery.join().unwrap(), outcome)
    });
    let OrdinaryTurnExecutionOutcome::Incomplete { reason } = outcome else {
        panic!("steering correlation loss did not converge as incomplete: {outcome:?}")
    };
    assert!(matches!(reason, OrdinaryTurnCaptureLoss::TargetClosed(_)));
    assert_eq!(lifecycle.calls, 0);
    assert_eq!(branch.calls, 0);
    assert_projection_loss(&fixture, submitted.turn, accepted_input);

    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

const STEERING_TEXT: &str = "phase52 production steering";

fn prepare_steering_draft(fixture: &Fixture) {
    let prepared = PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(STEERING_TEXT).unwrap()]).unwrap(),
    )
    .unwrap();
    stage_prepared_content(&fixture.store, fixture.storage, &prepared);
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) = DraftPayloadUpdate::prepare(
        &current,
        &prepared,
        SyndicTimestamp::from_unix_millis(52_001),
    )
    .unwrap() else {
        panic!("the steering fixture draft must change")
    };
    execute(
        &fixture.store,
        fixture
            .storage
            .update_draft_payload(fixture.storage.revision(&fixture.store).unwrap(), update),
    );
}

fn admit_claim_and_trigger(
    fixture: &Fixture,
    turn: SyndicTurnId,
    trigger: SteeringFailureTrigger,
) -> SyndicAcceptedInputId {
    wait_for_steerable_gate(fixture, turn);
    let current = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let turn_state = fixture
        .storage
        .turn_state(&fixture.store, turn, point_limit())
        .unwrap()
        .unwrap();
    assert!(
        matches!(gate.state(), InputGateState::Steerable(actual) if *actual == turn),
        "ordinary activation must expose the exact turn as steerable"
    );
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([154; 16]),
        None,
        turn_state.updated_at().max(current.draft().updated_at()),
    );
    let accepted_input = admission.accepted_input_id();
    let prepared = prepare_accepted_input_admission(
        &fixture.store,
        fixture.storage,
        fixture.state.assets(),
        admission,
    )
    .unwrap();
    fixture
        .store
        .execute_accepted_input_admission(prepared)
        .unwrap();

    wait_for_delivering_input(fixture, accepted_input);
    trigger.send(correlation(accepted_input));
    accepted_input
}

fn wait_for_delivering_input(fixture: &Fixture, accepted_input: SyndicAcceptedInputId) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if fixture
            .storage
            .delivering_steering_input(&fixture.store, accepted_input, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "automatic steering did not claim the accepted input"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_steerable_gate(fixture: &Fixture, turn: SyndicTurnId) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let state = fixture
            .storage
            .turn_state(&fixture.store, turn, point_limit())
            .unwrap()
            .unwrap();
        if matches!(gate.state(), InputGateState::Steerable(actual) if *actual == turn)
            && state.source_event_count() >= 3
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "ordinary activation did not become durably steerable"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn correlation(input: SyndicAcceptedInputId) -> String {
    let mut encoded = String::with_capacity("beryl.accepted-input.v1:".len() + 32);
    encoded.push_str("beryl.accepted-input.v1:");
    for byte in input.as_bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn assert_projection_loss(
    fixture: &Fixture,
    turn: SyndicTurnId,
    accepted_input: SyndicAcceptedInputId,
) {
    assert!(
        fixture
            .storage
            .delivering_steering_input(&fixture.store, accepted_input, point_limit())
            .unwrap()
            .is_none(),
        "projection loss must remove delivering eligibility"
    );
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let route = gate
        .selected_route()
        .expect("projection loss must retain accepted-input history");
    let page = fixture
        .storage
        .accepted_route_page(
            &fixture.store,
            fixture.thread,
            route.generation(),
            route.revision(),
            None,
        )
        .unwrap();
    let entry = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == accepted_input)
        .expect("projection loss must retain the accepted input");
    assert_eq!(
        entry.effective_state(),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    let state = fixture
        .storage
        .turn_state(&fixture.store, turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::StreamLost)
    );
}

fn stage_prepared_content(
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
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

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed { later_failure: None, .. } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => panic!("expected committed command, got {outcome:?}"),
        outcome @ CommandOutcome::Committed { later_failure: Some(_), .. } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => panic!("expected committed command, got {outcome:?}"),
    }
}
