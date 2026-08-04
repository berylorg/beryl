#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{DomainCallbackSource, DomainRegistrationError, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicItemId,
    ThreadRevision,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord, inject_retired_accepted_input_v2};
use syndic_storage::*;

use support::{
    TestHome, batch, commit, draft_id, empty_composer_content,
    exact_cas::submit_current_draft,
    id, open,
    phase11::{abandonment_request, delivering_input, mixed_abandonment_records},
    populated::{active_snapshot, active_turn, next_input, populated_records, steering_input},
    stage_prepared_content, timestamp,
};

#[derive(Clone, Copy)]
enum ActiveGateCase {
    Awaiting,
    Steerable,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn seeded(name: &str, records: Vec<FixtureRecord>) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(records));
    (home, store, storage)
}

fn active_records(case: ActiveGateCase) -> Vec<FixtureRecord> {
    let mut records = populated_records();
    let thread = id(40);
    let turn = active_turn();
    for record in &mut records {
        match record {
            FixtureRecord::InputGate(gate) if gate.thread_id() == thread => {
                let state = match case {
                    ActiveGateCase::Awaiting => InputGateState::AwaitingSteering(turn),
                    ActiveGateCase::Steerable => InputGateState::Steerable(turn),
                };
                *gate = InputGateRecord::new(
                    gate.thread_id(),
                    gate.revision(),
                    state,
                    gate.accepted_high_water(),
                    gate.route_generation_high_water(),
                    gate.selected_route(),
                    gate.live_steering_count(),
                    gate.live_next_turn_count(),
                    gate.live_logical_utf8_bytes(),
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteGeneration(route)
                if route.thread_id() == thread && matches!(case, ActiveGateCase::Awaiting) =>
            {
                let AcceptedRouteTarget::Steering(target) = route.target() else {
                    panic!("active fixture must have a steering target");
                };
                *route = AcceptedRouteGenerationRecord::new(
                    route.thread_id(),
                    route.generation(),
                    route.revision(),
                    AcceptedRouteTarget::AwaitingSteering(target.pending().clone()),
                    route.first_ordinal(),
                    route.last_ordinal(),
                    route.input_count(),
                    route.ready_retryable_count(),
                    route.delivering_count(),
                    route.next_turn_count(),
                    route.terminal_count(),
                    route.live_logical_utf8_bytes(),
                    route.delivering_logical_utf8_bytes(),
                )
                .unwrap();
            }
            _ => {}
        }
    }
    if !matches!(case, ActiveGateCase::Steerable) {
        records.retain(|record| {
            !matches!(
                record,
                FixtureRecord::AcceptedReadySource(source) if source.thread_id() == thread
            )
        });
    }
    records
}

fn select_compacting_route(store: &HomeStore, storage: SyndicStorage) {
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let proof =
        AcceptedRouteHeadProof::new(AcceptedRouteGeneration::FIRST, AcceptedRouteRevision::FIRST);
    let InputGateState::PendingTurn(turn) = gate.state() else {
        panic!("compacting fixture must begin from a pending turn");
    };
    let replacement = InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        InputGateState::Compacting {
            turn_id: *turn,
            operation_nonce: CompactionOperationNonce::from_bytes([57; 16]),
        },
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        Some(proof),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut fixture = FixtureBatch::new();
    fixture.put(FixtureRecord::InputGate(replacement)).unwrap();
    fixture
        .put(FixtureRecord::AcceptedRouteGenerationHead(
            AcceptedRouteGenerationHeadRecord::new(id(40), proof),
        ))
        .unwrap();
    commit(store, storage, fixture);
}

fn save_text(store: &HomeStore, storage: SyndicStorage, text: &str, updated_at: SyndicTimestamp) {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &prepared);
    let current = storage
        .current_draft(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&current, &prepared, updated_at).unwrap()
    else {
        panic!("test payload must update the current draft");
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
}

fn admission(
    store: &HomeStore,
    storage: SyndicStorage,
    next_draft: u8,
    admitted_at: SyndicTimestamp,
) -> AcceptedInputAdmission {
    let current = storage
        .current_draft(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    AcceptedInputAdmission::new(
        id(40),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        draft_id(next_draft),
        None,
        admitted_at,
    )
}

fn commit_admission(store: &HomeStore, storage: SyndicStorage, admission: &AcceptedInputAdmission) {
    assert_eq!(
        storage
            .accepted_input_status(store, admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    execute(
        store,
        storage.admit_accepted_input(storage.revision(store).unwrap(), admission.clone()),
    );
    assert_eq!(
        storage
            .accepted_input_status(store, admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    store.validate_registered_domains().unwrap();
}

fn seed_pending_gate(name: &str) -> (TestHome, HomeStore, SyndicStorage) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                id(40),
                draft_id(41),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    submit_current_draft(
        &store,
        storage,
        id(40),
        draft_id(42),
        SyndicItemId::from_bytes([43; 16]),
        "initial turn",
        timestamp(2),
    );
    store.validate_registered_domains().unwrap();
    (home, store, storage)
}

#[test]
fn active_admission_reconciliation_accepts_awaiting_and_steerable_successors() {
    for (name, case) in [
        (
            "phase57-admission-reconcile-awaiting",
            ActiveGateCase::Awaiting,
        ),
        (
            "phase57-admission-reconcile-steerable",
            ActiveGateCase::Steerable,
        ),
    ] {
        let (_home, store, storage) = seeded(name, active_records(case));
        store.validate_registered_domains().unwrap();
        save_text(&store, storage, "active admission", timestamp(20));
        let admission = admission(&store, storage, 90, timestamp(21));
        commit_admission(&store, storage, &admission);
    }
}

#[test]
fn next_turn_admission_reconciliation_accepts_an_existing_pending_gate() {
    let (_home, store, storage) = seed_pending_gate("phase57-admission-reconcile-pending");
    store.validate_registered_domains().unwrap();
    save_text(&store, storage, "next admission", timestamp(20));
    let admission = admission(&store, storage, 91, timestamp(21));
    commit_admission(&store, storage, &admission);
}

#[test]
fn compacting_gate_without_its_selected_operation_is_rejected() {
    let (_home, store, storage) = seed_pending_gate("phase57-compacting-missing-operation");
    save_text(&store, storage, "seed next route", timestamp(3));
    let seed = admission(&store, storage, 44, timestamp(4));
    commit_admission(&store, storage, &seed);
    select_compacting_route(&store, storage);

    let error = store.validate_registered_domains().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("compacting gate operation is missing")
    );
}

#[test]
fn later_admission_preserves_the_original_exact_receipt() {
    let (_home, store, storage) = seeded(
        "phase57-admission-reconcile-drift",
        active_records(ActiveGateCase::Steerable),
    );
    save_text(&store, storage, "first admission", timestamp(20));
    let first = admission(&store, storage, 92, timestamp(21));
    commit_admission(&store, storage, &first);

    save_text(&store, storage, "second admission", timestamp(22));
    let second = admission(&store, storage, 93, timestamp(23));
    commit_admission(&store, storage, &second);

    assert_eq!(
        storage
            .accepted_input_status(&store, &first, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    store.validate_registered_domains().unwrap();
}

#[test]
fn complete_receipt_discriminators_reject_same_natural_identity_collisions() {
    let (_home, store, storage) = seeded(
        "phase57-admission-reconcile-discriminators",
        active_records(ActiveGateCase::Steerable),
    );
    save_text(&store, storage, "receipt discriminators", timestamp(20));
    let original = admission(&store, storage, 94, timestamp(21));
    commit_admission(&store, storage, &original);

    let variants = [
        AcceptedInputAdmission::new(
            original.thread_id(),
            original.expected_thread_revision().checked_next().unwrap(),
            original.draft_id(),
            original.expected_draft_revision(),
            original.expected_content(),
            original.expected_gate_revision(),
            original.next_draft_id(),
            original.asset_reference_set(),
            original.admitted_at(),
        ),
        AcceptedInputAdmission::new(
            original.thread_id(),
            original.expected_thread_revision(),
            original.draft_id(),
            original.expected_draft_revision().checked_next().unwrap(),
            original.expected_content(),
            original.expected_gate_revision(),
            original.next_draft_id(),
            original.asset_reference_set(),
            original.admitted_at(),
        ),
        AcceptedInputAdmission::new(
            original.thread_id(),
            original.expected_thread_revision(),
            original.draft_id(),
            original.expected_draft_revision(),
            original.expected_content(),
            original.expected_gate_revision().checked_next().unwrap(),
            original.next_draft_id(),
            original.asset_reference_set(),
            original.admitted_at(),
        ),
        AcceptedInputAdmission::new(
            original.thread_id(),
            original.expected_thread_revision(),
            original.draft_id(),
            original.expected_draft_revision(),
            original.expected_content(),
            original.expected_gate_revision(),
            draft_id(95),
            original.asset_reference_set(),
            original.admitted_at(),
        ),
    ];
    for variant in variants {
        assert_eq!(
            storage
                .accepted_input_status(&store, &variant, point_limit())
                .unwrap(),
            InputAdmissionStatus::Collision
        );
    }
}

#[test]
fn accepted_input_v3_receipt_reopens_and_v2_is_rejected() {
    let (home, store, storage) = seeded(
        "phase57-admission-reconcile-v3",
        active_records(ActiveGateCase::Steerable),
    );
    save_text(&store, storage, "v3 receipt", timestamp(20));
    let admission = admission(&store, storage, 96, timestamp(21));
    commit_admission(&store, storage, &admission);
    store.close().unwrap();

    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_status(&reopened, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    reopened.close().unwrap();

    let retired_home = TestHome::new("phase57-admission-reconcile-v2-rejected");
    let mut retired_store = open(retired_home.path());
    let retired_storage = SyndicStorage::register(&mut retired_store).unwrap();
    inject_retired_accepted_input_v2(&retired_store, retired_storage).unwrap();
    retired_store.close().unwrap();
    let mut retired_reopen = open(retired_home.path());
    assert!(matches!(
        SyndicStorage::register(&mut retired_reopen),
        Err(DomainRegistrationError::ValidationAccess {
            domain: "syndic",
            source: DomainCallbackSource::Read(_),
        })
    ));
    retired_reopen.close().unwrap();
}

fn transition_facts(
    store: &HomeStore,
    storage: SyndicStorage,
    input: SyndicAcceptedInputId,
) -> (SteeringTargetProof, AcceptedInputRevision) {
    if let Some(ready) = storage
        .ready_steering_input(store, input, point_limit())
        .unwrap()
    {
        return (ready.target().clone(), ready.accepted_input_revision());
    }
    let delivering = storage
        .delivering_steering_input(store, input, point_limit())
        .unwrap()
        .expect("transition input must be ready or delivering");
    (
        delivering.target().clone(),
        delivering.accepted_input_revision(),
    )
}

#[derive(Clone, Copy)]
enum DeliveryRace {
    Begin,
    Retry,
    Complete,
    Reject,
}

impl DeliveryRace {
    const ALL: [Self; 4] = [Self::Begin, Self::Retry, Self::Complete, Self::Reject];

    const fn name(self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Retry => "retry",
            Self::Complete => "complete",
            Self::Reject => "reject",
        }
    }

    fn records(self) -> Vec<FixtureRecord> {
        match self {
            Self::Begin => active_records(ActiveGateCase::Steerable),
            Self::Retry | Self::Complete | Self::Reject => mixed_abandonment_records(),
        }
    }

    fn input(self) -> SyndicAcceptedInputId {
        match self {
            Self::Begin => steering_input(),
            Self::Retry | Self::Complete | Self::Reject => delivering_input(),
        }
    }

    fn request(self, store: &HomeStore, storage: SyndicStorage) -> StableDeliveryRequest {
        let input = self.input();
        let (target, revision) = transition_facts(store, storage, input);
        match self {
            Self::Begin => StableDeliveryRequest::Begin(BeginAcceptedInputDelivery::new(
                id(40),
                input,
                revision,
                target,
            )),
            Self::Retry => StableDeliveryRequest::Retry(RetryAcceptedInputDelivery::new(
                id(40),
                input,
                revision,
                target,
            )),
            Self::Complete => StableDeliveryRequest::Complete(CompleteAcceptedInputDelivery::new(
                id(40),
                input,
                revision,
                target,
            )),
            Self::Reject => StableDeliveryRequest::Reject(SteeringRejection::new(
                id(40),
                input,
                revision,
                target,
            )),
        }
    }

    const fn transition_kind(self) -> AcceptedRouteLeafTransitionKind {
        match self {
            Self::Begin => AcceptedRouteLeafTransitionKind::Begin,
            Self::Retry => AcceptedRouteLeafTransitionKind::Retry,
            Self::Complete => AcceptedRouteLeafTransitionKind::Complete,
            Self::Reject => AcceptedRouteLeafTransitionKind::SteeringRejected,
        }
    }
}

#[derive(Clone)]
enum StableDeliveryRequest {
    Begin(BeginAcceptedInputDelivery),
    Retry(RetryAcceptedInputDelivery),
    Complete(CompleteAcceptedInputDelivery),
    Reject(SteeringRejection),
}

impl StableDeliveryRequest {
    fn status(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
    ) -> AcceptedInputDeliveryTransitionStatus {
        match self {
            Self::Begin(request) => {
                storage.begin_accepted_input_delivery_status(store, request, point_limit())
            }
            Self::Retry(request) => {
                storage.retry_accepted_input_delivery_status(store, request, point_limit())
            }
            Self::Complete(request) => {
                storage.complete_accepted_input_delivery_status(store, request, point_limit())
            }
            Self::Reject(request) => {
                storage.steering_rejection_status(store, request, point_limit())
            }
        }
        .unwrap()
    }

    fn execute(&self, store: &HomeStore, storage: SyndicStorage) {
        let command = match self {
            Self::Begin(request) => storage.current_begin_accepted_input_delivery(request.clone()),
            Self::Retry(request) => storage.current_retry_accepted_input_delivery(request.clone()),
            Self::Complete(request) => {
                storage.current_complete_accepted_input_delivery(request.clone())
            }
            Self::Reject(request) => storage.current_record_steering_rejection(request.clone()),
        };
        store.execute_current(command).unwrap();
    }

    const fn source_revision(&self) -> AcceptedInputRevision {
        match self {
            Self::Begin(request) => request.expected_input_revision(),
            Self::Retry(request) => request.expected_input_revision(),
            Self::Complete(request) => request.expected_input_revision(),
            Self::Reject(request) => request.expected_input_revision(),
        }
    }
}

fn route_leaf(
    store: &HomeStore,
    storage: SyndicStorage,
    input: SyndicAcceptedInputId,
) -> AcceptedRouteLeafRecord {
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let route = gate.selected_route().unwrap();
    storage
        .accepted_route_page(store, id(40), route.generation(), route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == input)
        .unwrap()
        .leaf()
        .clone()
}

#[test]
fn sibling_admission_between_snapshot_and_delivery_transition_preserves_stable_intent() {
    for operation in DeliveryRace::ALL {
        let name = format!("phase57-sibling-delivery-{}", operation.name());
        let (_home, store, storage) = seeded(&name, operation.records());
        let request = operation.request(&store, storage);

        save_text(&store, storage, "pre-transition sibling", timestamp(30));
        let sibling = admission(&store, storage, 110, timestamp(31));
        commit_admission(&store, storage, &sibling);
        let source_gate = storage
            .input_gate(&store, id(40), point_limit())
            .unwrap()
            .unwrap();
        let source_route = source_gate.selected_route().unwrap();
        assert_eq!(
            request.status(&store, storage),
            AcceptedInputDeliveryTransitionStatus::Prior
        );

        request.execute(&store, storage);
        assert_eq!(
            request.status(&store, storage),
            AcceptedInputDeliveryTransitionStatus::Exact
        );
        assert_eq!(
            route_leaf(&store, storage, operation.input()).last_transition(),
            Some(AcceptedRouteLeafTransitionProof::new(
                source_gate.revision(),
                source_route,
                request.source_revision(),
                operation.transition_kind(),
            ))
        );

        save_text(&store, storage, "post-transition sibling", timestamp(32));
        let descendant = admission(&store, storage, 111, timestamp(33));
        commit_admission(&store, storage, &descendant);
        assert_eq!(
            request.status(&store, storage),
            AcceptedInputDeliveryTransitionStatus::Exact,
            "a compatible sibling descendant must not require a quiet shared revision",
        );
        store.validate_registered_domains().unwrap();
    }
}

#[test]
fn sibling_admission_between_snapshot_and_abandonment_preserves_stable_intent() {
    for named in [false, true] {
        let suffix = if named { "named" } else { "generic" };
        let name = format!("phase57-sibling-abandonment-{suffix}");
        let (_home, store, storage) = seeded(&name, mixed_abandonment_records());
        let generic = abandonment_request(&store, storage);
        let request = if named {
            AbandonActiveBinding::after_exact_rejection(
                generic.thread_id(),
                generic.expected_binding_revision(),
                generic.route_generation(),
                generic.target().clone(),
                generic.selected_path(),
                generic.stale().clone(),
                ExactRejectedInputDelivery::new(
                    delivering_input(),
                    AcceptedInputRevision::new(2).unwrap(),
                ),
            )
        } else {
            generic
        };

        save_text(&store, storage, "pre-abandonment sibling", timestamp(40));
        let sibling = admission(&store, storage, 112, timestamp(41));
        commit_admission(&store, storage, &sibling);
        let source_gate = storage
            .input_gate(&store, id(40), point_limit())
            .unwrap()
            .unwrap();
        let source_route = source_gate.selected_route().unwrap();
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, &request, point_limit())
                .unwrap(),
            BindingPublicationStatus::Prior
        );

        store
            .execute_current(storage.current_abandon_active_binding(request.clone()))
            .unwrap();
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, &request, point_limit())
                .unwrap(),
            BindingPublicationStatus::Exact
        );
        if named {
            assert_eq!(
                route_leaf(&store, storage, delivering_input()).last_transition(),
                Some(AcceptedRouteLeafTransitionProof::new(
                    source_gate.revision(),
                    source_route,
                    AcceptedInputRevision::new(2).unwrap(),
                    AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
                ))
            );
        }

        save_text(&store, storage, "post-abandonment sibling", timestamp(42));
        let descendant = admission(&store, storage, 113, timestamp(43));
        commit_admission(&store, storage, &descendant);
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, &request, point_limit())
                .unwrap(),
            BindingPublicationStatus::Exact,
            "a compatible sibling descendant must preserve abandonment evidence",
        );
        store.validate_registered_domains().unwrap();
    }
}

fn remove_transition_witness(leaf: &AcceptedRouteLeafRecord) -> AcceptedRouteLeafRecord {
    AcceptedRouteLeafRecord::new(
        leaf.input_id(),
        leaf.thread_id(),
        leaf.generation(),
        leaf.ordinal(),
        leaf.revision(),
        leaf.state(),
        leaf.lifecycle(),
    )
}

fn assert_missing_transition_witness_rejected(
    home: &TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    input: SyndicAcceptedInputId,
) {
    let leaf = route_leaf(&store, storage, input);
    assert!(leaf.last_transition().is_some());
    commit(
        &store,
        storage,
        batch(vec![FixtureRecord::AcceptedRouteLeaf(
            remove_transition_witness(&leaf),
        )]),
    );
    let error = store.validate_registered_domains().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("transitioned accepted-route leaf is missing its witness")
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("transitioned leaf without a witness survived reopen"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("transitioned accepted-route leaf is missing its witness")
    );
    reopened.close().unwrap();
}

#[test]
fn every_delivery_successor_requires_its_transition_witness_on_validation_and_reopen() {
    for operation in DeliveryRace::ALL {
        let name = format!("phase57-missing-delivery-witness-{}", operation.name());
        let (home, store, storage) = seeded(&name, operation.records());
        operation.request(&store, storage).execute(&store, storage);
        assert_missing_transition_witness_rejected(&home, store, storage, operation.input());
    }
}

#[test]
fn exact_projection_loss_successor_requires_its_transition_witness() {
    let (home, store, storage) = seeded(
        "phase57-missing-projection-loss-witness",
        mixed_abandonment_records(),
    );
    let generic = abandonment_request(&store, storage);
    let request = AbandonActiveBinding::after_exact_rejection(
        generic.thread_id(),
        generic.expected_binding_revision(),
        generic.route_generation(),
        generic.target().clone(),
        generic.selected_path(),
        generic.stale().clone(),
        ExactRejectedInputDelivery::new(delivering_input(), AcceptedInputRevision::new(2).unwrap()),
    );
    store
        .execute_current(storage.current_abandon_active_binding(request))
        .unwrap();
    assert_missing_transition_witness_rejected(&home, store, storage, delivering_input());
}

#[test]
fn legal_delivery_descendants_preserve_exact_admission_reconciliation() {
    for (name, disposition) in [
        ("phase57-admission-descendant-retry", "retry"),
        ("phase57-admission-descendant-complete", "complete"),
        ("phase57-admission-descendant-rejection", "rejection"),
    ] {
        let (_home, store, storage) = seeded(name, active_records(ActiveGateCase::Steerable));
        save_text(&store, storage, "route descendant", timestamp(20));
        let admission = admission(&store, storage, 97, timestamp(21));
        commit_admission(&store, storage, &admission);
        let input = admission.accepted_input_id();
        let (target, revision) = transition_facts(&store, storage, input);
        execute(
            &store,
            storage.begin_accepted_input_delivery(
                storage.revision(&store).unwrap(),
                BeginAcceptedInputDelivery::new(id(40), input, revision, target),
            ),
        );
        assert_eq!(
            storage
                .accepted_input_status(&store, &admission, point_limit())
                .unwrap(),
            InputAdmissionStatus::ExactAccepted
        );

        let (target, revision) = transition_facts(&store, storage, input);
        let contribution = match disposition {
            "retry" => storage.retry_accepted_input_delivery(
                storage.revision(&store).unwrap(),
                RetryAcceptedInputDelivery::new(id(40), input, revision, target.clone()),
            ),
            "complete" => storage.complete_accepted_input_delivery(
                storage.revision(&store).unwrap(),
                CompleteAcceptedInputDelivery::new(id(40), input, revision, target.clone()),
            ),
            "rejection" => storage.record_steering_rejection(
                storage.revision(&store).unwrap(),
                SteeringRejection::new(id(40), input, revision, target),
            ),
            _ => unreachable!(),
        };
        execute(&store, contribution);
        assert_eq!(
            storage
                .accepted_input_status(&store, &admission, point_limit())
                .unwrap(),
            InputAdmissionStatus::ExactAccepted
        );
        store.validate_registered_domains().unwrap();
    }
}

#[test]
fn projection_loss_descendant_preserves_exact_reconciliation() {
    let (_home, store, storage) = seeded(
        "phase57-admission-descendant-projection-loss",
        active_records(ActiveGateCase::Steerable),
    );
    save_text(&store, storage, "projection-loss descendant", timestamp(20));
    let projection_loss_admission = admission(&store, storage, 99, timestamp(21));
    commit_admission(&store, storage, &projection_loss_admission);
    let binding = storage
        .current_binding(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding must be active");
    };
    let snapshot = storage
        .execution_snapshot(&store, active_snapshot(), point_limit())
        .unwrap()
        .unwrap();
    let ready = storage
        .ready_steering_input(
            &store,
            projection_loss_admission.accepted_input_id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        "focused projection loss",
        timestamp(22),
    )
    .unwrap();
    execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                id(40),
                binding.binding().revision(),
                ready.route().generation(),
                AcceptedRouteLostTarget::Steering(ready.target().clone()),
                binding.binding().selected_path(),
                stale,
            ),
        ),
    );
    assert_eq!(
        storage
            .accepted_input_status(&store, &projection_loss_admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    store.validate_registered_domains().unwrap();
}

#[test]
fn receipt_construction_and_reopen_validation_reject_corrupt_identity_chains() {
    let source = draft_id(97);
    assert_eq!(
        AcceptedInputAdmissionProof::new(
            ThreadRevision::new(1).unwrap(),
            source,
            DraftRevision::new(1).unwrap(),
            InputGateRevision::new(1).unwrap(),
            source,
        ),
        Err(SyndicRecordError::AcceptedInputAdmissionDraftCollision)
    );
    let proof = AcceptedInputAdmissionProof::new(
        ThreadRevision::new(1).unwrap(),
        source,
        DraftRevision::new(1).unwrap(),
        InputGateRevision::new(1).unwrap(),
        draft_id(98),
    )
    .unwrap();
    assert!(matches!(
        AcceptedInputRecord::new(
            SyndicAcceptedInputId::from_bytes([99; 16]),
            id(40),
            AcceptedInputOrdinal::FIRST,
            proof,
            AcceptedRouteGeneration::FIRST,
            empty_composer_content(),
            None,
            timestamp(1),
        ),
        Err(SyndicRecordError::AcceptedInputIdentityMismatch)
    ));

    let mut records = active_records(ActiveGateCase::Steerable);
    let input = records
        .iter_mut()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == next_input() => Some(input),
            _ => None,
        })
        .unwrap();
    let proof = input.admission();
    *input = AcceptedInputRecord::new(
        input.id(),
        input.thread_id(),
        input.ordinal(),
        AcceptedInputAdmissionProof::new(
            proof.expected_thread_revision(),
            proof.source_draft_id(),
            proof.expected_draft_revision(),
            proof.expected_gate_revision(),
            draft_id(99),
        )
        .unwrap(),
        input.route_generation(),
        input.content(),
        input.asset_reference_set(),
        input.admitted_at(),
    )
    .unwrap();
    let (_home, store, _storage) = seeded("phase57-admission-corrupt-descendant", records);
    let error = store.validate_registered_domains().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("accepted-input replacement descendant is not exclusive")
    );
}
