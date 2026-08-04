use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasThreadId, CasTurnId, InputGateRevision, SyndicDraftId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    ContentAppend, ContentBuild, CreateThread, PreparedContent, SourceEventPayload,
    StopAdmissionIneligibility, StopAdmissionRead, StopCause, StopOperationTarget,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TurnEndStatus,
};

use super::*;
use crate::{
    LifecycleYieldOutcome,
    cas_projection::connection::{TargetTurnRegistration, registry::LoadedThreadKey},
};

#[allow(dead_code)]
mod exact_cas {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../syndic-storage/tests/support/exact_cas.rs"
    ));
}

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(home: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    home.execute(command).unwrap();
}

fn stage_prepared_content(home: &HomeStore, storage: SyndicStorage, content: &PreparedContent) {
    execute(
        home,
        storage.begin_content(
            storage.revision(home).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            home,
            storage.append_content(storage.revision(home).unwrap(), append),
        );
        manifest = next;
    }
}

struct StopFixture {
    _directory: tempfile::TempDir,
    home: Arc<HomeStore>,
    storage: SyndicStorage,
    coordinator: Arc<StopCoordinator>,
    command_gate: crate::cas_projection::persistent_failure::MasterCommandGate,
    router: Arc<EventRouter>,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    target: StopOperationTarget,
    proof: StopTargetProof,
}

impl StopFixture {
    fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut home).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute(
            &home,
            storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    exact_cas::execution_binding(),
                    timestamp(1),
                ),
            ),
        );
        let turn = exact_cas::submit_current_draft(
            &home,
            storage,
            thread,
            SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]),
            SyndicItemId::from_bytes([seed.wrapping_add(3); 16]),
            "coordinator stop target",
            timestamp(2),
        );
        let source = exact_cas::establish_turn(&home, storage, thread, turn, timestamp(3));
        exact_cas::admit_event(
            &home,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        let target = match storage
            .stop_admission_read(&home, thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Admissible(candidate) => candidate.target().clone(),
            other => panic!("active fixture must admit a stop, observed {other:?}"),
        };
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let home = Arc::new(home);
        let command_gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
            crate::cas_projection::persistent_failure::ProjectionServiceGeneration::allocate()
                .unwrap(),
            None,
        );
        let coordinator = Arc::new(StopCoordinator::new(
            &home,
            home_id,
            home_generation,
            storage,
            command_gate.authorizer(),
        ));
        let router = Arc::new(
            EventRouter::new(
                target.runtime_id(),
                target.loaded_generation().process(),
                NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed),
            )
            .unwrap(),
        );
        let router_command = router.authorize_command_for_test().unwrap();
        router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: target.runtime_id(),
                    process_generation: target.loaded_generation().process(),
                    cas_thread_id: target.cas_thread_id().clone(),
                },
                thread,
                target.loaded_generation(),
                home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Active(target.cas_turn_id().clone()),
            )
            .unwrap();
        drop(router_command);
        let proof = router
            .stop_target(thread, target.cas_thread_id(), target.cas_turn_id())
            .unwrap();
        Self {
            _directory: directory,
            home,
            storage,
            coordinator,
            command_gate,
            router,
            thread,
            turn,
            target,
            proof,
        }
    }

    fn wrong_storage_target_proof(&self, seed: u8) -> StopTargetProof {
        let cas_thread = CasThreadId::new(format!("wrong-stop-thread-{seed}")).unwrap();
        let cas_turn = CasTurnId::new(format!("wrong-stop-turn-{seed}")).unwrap();
        let router_command = self.router.authorize_command_for_test().unwrap();
        self.router
            .register(
                &router_command,
                LoadedThreadKey {
                    runtime_id: self.target.runtime_id(),
                    process_generation: self.target.loaded_generation().process(),
                    cas_thread_id: cas_thread.clone(),
                },
                self.thread,
                self.target.loaded_generation(),
                self.home.health().generation().unwrap().get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Active(cas_turn.clone()),
            )
            .unwrap();
        drop(router_command);
        self.router
            .stop_target(self.thread, &cas_thread, &cas_turn)
            .unwrap()
    }

    fn live_stop(&self) -> syndic_storage::SyndicLiveStopOperation {
        match self
            .storage
            .stop_admission_read(&self.home, self.thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Stopping(live) => *live,
            other => panic!("fixture must retain a live stop, observed {other:?}"),
        }
    }
}

fn failure_identity(
    fixture: &StopFixture,
) -> crate::cas_projection::persistent_failure::PersistentFailureCutIdentity {
    crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
        fixture.home.home_id(),
        fixture.home.health().generation().unwrap(),
        fixture.command_gate.service_generation(),
        crate::cas_projection::persistent_failure::PersistentFailureGeneration::FIRST,
    )
}

#[test]
fn dropping_claimed_stop_owner_preserves_durable_claim_without_home_io() {
    let fixture = StopFixture::new(29);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id();
    let revision = fixture.home.home_revision().unwrap();

    drop(owner);

    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let state = fixture.coordinator.state.lock().unwrap();
    let local = state.stops.get(&fixture.thread).unwrap();
    assert_eq!(local.operation_id, operation_id);
    assert_eq!(local.dispatch, LocalDispatchState::ClaimUnresolved);
    drop(state);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn dropping_dispatching_stop_owner_widens_ambiguity_without_home_io() {
    let fixture = StopFixture::new(30);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    owner.begin_dispatch().unwrap();
    let operation_id = owner.operation_id();
    let revision = fixture.home.home_revision().unwrap();

    drop(owner);

    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let state = fixture.coordinator.state.lock().unwrap();
    let local = state.stops.get(&fixture.thread).unwrap();
    assert_eq!(local.operation_id, operation_id);
    assert_eq!(local.dispatch, LocalDispatchState::PossiblyDispatched);
    drop(state);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn router_valid_proof_for_the_wrong_storage_target_is_rejected() {
    let fixture = StopFixture::new(31);
    let wrong = fixture.wrong_storage_target_proof(31);

    assert!(matches!(
        fixture
            .coordinator
            .coordinate(&fixture.router, wrong, StopCause::SelectedOperationControl,),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn matching_causes_join_one_primary_and_each_new_operation_gets_a_new_attempt() {
    let fixture = StopFixture::new(41);
    let first_owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first request must own dispatch"),
        Err(error) => panic!("first stop coordination failed: {error}"),
    };
    let first_operation = first_owner.operation_id;
    let first_attempt = first_owner.attempt;

    let joined_operation = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::DiagnosticControl,
    ) {
        Ok(StopOwnership::Joined { operation_id, .. }) => operation_id,
        Ok(StopOwnership::Primary(_)) => panic!("matching cause must not own a second dispatch"),
        Err(error) => panic!("cause join failed: {error}"),
    };
    assert_eq!(joined_operation, first_operation);
    let joined = fixture.live_stop();
    assert!(
        joined
            .record()
            .causes()
            .contains(StopCause::SelectedOperationControl)
    );
    assert!(
        joined
            .record()
            .causes()
            .contains(StopCause::DiagnosticControl)
    );
    assert!(matches!(
        first_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(operation_id)
            if operation_id == first_operation
    ));

    let second_owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("reopened operation must admit a new primary"),
        Err(error) => panic!("second stop coordination failed: {error}"),
    };
    assert_ne!(second_owner.operation_id, first_operation);
    assert_ne!(second_owner.attempt, first_attempt);
    assert_ne!(
        second_owner.operation_id.nonce().as_bytes(),
        second_owner.attempt.as_bytes()
    );
    assert!(matches!(
        second_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
}

#[test]
fn proven_nondispatch_reopens_without_approval_but_approval_ownership_abandons() {
    let safe = StopFixture::new(51);
    let safe_owner = match safe.coordinator.coordinate(
        &safe.router,
        safe.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first safe stop must own dispatch"),
        Err(error) => panic!("safe stop coordination failed: {error}"),
    };
    assert!(matches!(
        safe_owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
    assert!(matches!(
        safe.storage
            .stop_admission_read(&safe.home, safe.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));

    let approval = StopFixture::new(52);
    let primary = match approval.coordinator.coordinate(
        &approval.router,
        approval.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first approval fixture stop must own dispatch"),
        Err(error) => panic!("approval fixture coordination failed: {error}"),
    };
    let operation_id = primary.operation_id;
    assert!(matches!(
        approval.coordinator.coordinate(
            &approval.router,
            approval.proof.clone(),
            StopCause::InterruptingApproval,
        ),
        Ok(StopOwnership::Joined {
            operation_id: joined,
            ..
        }) if joined == operation_id
    ));
    assert!(matches!(
        primary.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Abandoned(abandoned) if abandoned == operation_id
    ));
    assert_eq!(
        approval
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .get(&approval.thread)
            .unwrap()
            .dispatch,
        LocalDispatchState::DurablyAbandoned
    );
    assert!(
        approval
            .coordinator
            .abandon_for_authority_loss(approval.thread, approval.turn)
            .unwrap()
    );
    assert!(
        !approval
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .contains_key(&approval.thread)
    );
}

#[test]
fn safe_reopen_requires_the_exact_local_and_durable_dispatch_authority() {
    let fixture = StopFixture::new(53);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id;
    let attempt = owner.attempt;

    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = None;
        local.dispatch = LocalDispatchState::AdmittedNotClaimed;
    }
    assert!(matches!(
        fixture.coordinator.settle_unclaimed(operation_id),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    let foreign_attempt = StopAttemptNonce::from_bytes([0xa5; 16]);
    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = Some(foreign_attempt);
        local.dispatch = LocalDispatchState::ClaimedNotDispatched;
    }
    assert!(matches!(
        fixture
            .coordinator
            .settle_proven_nondispatch(operation_id, Some(foreign_attempt)),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    {
        let mut state = fixture.coordinator.state.lock().unwrap();
        let local = state.stops.get_mut(&fixture.thread).unwrap();
        local.attempt = Some(attempt);
        local.dispatch = LocalDispatchState::ClaimUnresolved;
    }
    assert!(matches!(
        fixture
            .coordinator
            .settle_proven_nondispatch(operation_id, Some(attempt)),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    let claimed = fixture.live_stop();
    assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
    assert_eq!(claimed.attempt(), Some(attempt));

    fixture
        .coordinator
        .state
        .lock()
        .unwrap()
        .stops
        .get_mut(&fixture.thread)
        .unwrap()
        .dispatch = LocalDispatchState::ClaimedNotDispatched;
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
}

#[test]
fn persistent_failure_freezes_claimed_owner_without_durable_settlement() {
    let fixture = StopFixture::new(54);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id;
    let revision = fixture.home.home_revision().unwrap();
    let identity = failure_identity(&fixture);

    assert!(
        fixture
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    assert!(matches!(
        owner.begin_dispatch(),
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert_eq!(
        fixture
            .coordinator
            .persistent_failure_evidence(identity, fixture.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));

    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let durable = fixture.live_stop();
    assert_eq!(durable.state(), StopOperationState::DispatchClaimed);
    assert!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .contains_key(&fixture.thread)
    );
}

#[test]
fn persistent_failure_cut_and_stop_claim_have_deterministic_two_order_linearization() {
    let claim_first = StopFixture::new(192);
    let claim_pause = claim_first
        .coordinator
        .install_race_pause(StopRaceStage::ClaimFenceHeld);
    let coordinator = Arc::clone(&claim_first.coordinator);
    let router = Arc::clone(&claim_first.router);
    let proof = claim_first.proof.clone();
    let claim = std::thread::spawn(move || {
        coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl)
    });
    assert!(
        claim_pause.wait_until_reached(Duration::from_secs(10)),
        "claim-first coordinate did not reach the held stop fence"
    );
    let identity = failure_identity(&claim_first);
    assert!(
        claim_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    let freeze_coordinator = Arc::clone(&claim_first.coordinator);
    let (freeze_started_tx, freeze_started_rx) = mpsc::sync_channel(1);
    let freeze = std::thread::spawn(move || {
        freeze_started_tx.send(()).unwrap();
        freeze_coordinator.freeze_for_persistent_failure(identity)
    });
    freeze_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("claim-first failure freeze started while the claim fence was held");
    claim_pause.release();
    let owner = match claim.join().unwrap() {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("claim-first stop unexpectedly joined"),
        Err(error) => panic!("claim-first stop failed after winning its mutex fence: {error}"),
    };
    freeze.join().unwrap().unwrap();
    assert_eq!(
        claim_first
            .coordinator
            .persistent_failure_evidence(identity, claim_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
    assert_eq!(
        claim_first.live_stop().state(),
        StopOperationState::DispatchClaimed
    );

    let cut_first = StopFixture::new(193);
    let claim_pause = cut_first
        .coordinator
        .install_race_pause(StopRaceStage::BeforeClaimFence);
    let coordinator = Arc::clone(&cut_first.coordinator);
    let router = Arc::clone(&cut_first.router);
    let proof = cut_first.proof.clone();
    let claim = std::thread::spawn(move || {
        coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl)
    });
    assert!(
        claim_pause.wait_until_reached(Duration::from_secs(10)),
        "cut-first coordinate did not reach the pre-claim fence"
    );
    let identity = failure_identity(&cut_first);
    assert!(
        cut_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    cut_first
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    assert_eq!(
        cut_first
            .coordinator
            .persistent_failure_evidence(identity, cut_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::NoLocalStop
    );
    claim_pause.release();
    assert!(matches!(
        claim.join().unwrap(),
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert!(
        cut_first
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .is_empty()
    );
    assert!(matches!(
        cut_first
            .storage
            .stop_admission_read(&cut_first.home, cut_first.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn dispatch_winning_before_cut_is_retained_as_ambiguous() {
    let fixture = StopFixture::new(55);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let operation_id = owner.operation_id;
    owner.begin_dispatch().unwrap();
    let revision = fixture.home.home_revision().unwrap();
    let identity = failure_identity(&fixture);

    fixture
        .command_gate
        .elect_persistent_failure_for_test(identity.failure_generation)
        .unwrap();
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    let evidence = fixture
        .coordinator
        .persistent_failure_evidence(identity, fixture.thread)
        .unwrap();
    assert_eq!(evidence, PersistentFailureStopEvidence::Dispatching);
    assert!(!evidence.permits_volatile_interrupt());
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert_eq!(
        fixture.live_stop().state(),
        StopOperationState::DispatchClaimed
    );
}

#[test]
fn persistent_failure_cut_and_begin_dispatch_have_deterministic_two_order_linearization() {
    let dispatch_first = StopFixture::new(194);
    let owner = match dispatch_first.coordinator.coordinate(
        &dispatch_first.router,
        dispatch_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("dispatch-first stop unexpectedly joined"),
        Err(error) => panic!("dispatch-first stop coordination failed: {error}"),
    };
    let dispatch_pause = dispatch_first
        .coordinator
        .install_race_pause(StopRaceStage::BeginDispatchFenceHeld);
    let dispatch = std::thread::spawn(move || {
        let result = owner.begin_dispatch();
        (owner, result)
    });
    assert!(
        dispatch_pause.wait_until_reached(Duration::from_secs(10)),
        "dispatch-first owner did not reach the held dispatch fence"
    );
    let identity = failure_identity(&dispatch_first);
    assert!(
        dispatch_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    let freeze_coordinator = Arc::clone(&dispatch_first.coordinator);
    let (freeze_started_tx, freeze_started_rx) = mpsc::sync_channel(1);
    let freeze = std::thread::spawn(move || {
        freeze_started_tx.send(()).unwrap();
        freeze_coordinator.freeze_for_persistent_failure(identity)
    });
    freeze_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("dispatch-first failure freeze started while the dispatch fence was held");
    dispatch_pause.release();
    let (owner, dispatch_result) = dispatch.join().unwrap();
    dispatch_result.unwrap();
    freeze.join().unwrap().unwrap();
    assert_eq!(
        dispatch_first
            .coordinator
            .persistent_failure_evidence(identity, dispatch_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::Dispatching
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));

    let cut_first = StopFixture::new(195);
    let owner = match cut_first.coordinator.coordinate(
        &cut_first.router,
        cut_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("cut-first stop unexpectedly joined"),
        Err(error) => panic!("cut-first stop coordination failed: {error}"),
    };
    let dispatch_pause = cut_first
        .coordinator
        .install_race_pause(StopRaceStage::BeforeBeginDispatchFence);
    let dispatch = std::thread::spawn(move || {
        let result = owner.begin_dispatch();
        (owner, result)
    });
    assert!(
        dispatch_pause.wait_until_reached(Duration::from_secs(10)),
        "cut-first owner did not reach the pre-dispatch fence"
    );
    let identity = failure_identity(&cut_first);
    assert!(
        cut_first
            .command_gate
            .elect_persistent_failure_for_test(identity.failure_generation)
            .unwrap()
    );
    cut_first
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    dispatch_pause.release();
    let (owner, dispatch_result) = dispatch.join().unwrap();
    assert!(matches!(
        dispatch_result,
        Err(StopCoordinationError::HomeAuthorityLost)
    ));
    assert_eq!(
        cut_first
            .coordinator
            .persistent_failure_evidence(identity, cut_first.thread)
            .unwrap(),
        PersistentFailureStopEvidence::ClaimedNotDispatched
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
}

#[test]
fn persistent_failure_classifies_every_retained_stop_dispatch_state() {
    let fixture = StopFixture::new(56);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    let identity = failure_identity(&fixture);
    fixture
        .command_gate
        .elect_persistent_failure_for_test(identity.failure_generation)
        .unwrap();
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    let attempt = owner.attempt;

    let cases = [
        (
            LocalDispatchState::AdmittedNotClaimed,
            None,
            PersistentFailureStopEvidence::AdmittedNotClaimed,
            true,
        ),
        (
            LocalDispatchState::ClaimUnresolved,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimUnresolved,
            false,
        ),
        (
            LocalDispatchState::ClaimedNotDispatched,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimedNotDispatched,
            true,
        ),
        (
            LocalDispatchState::Dispatching,
            Some(attempt),
            PersistentFailureStopEvidence::Dispatching,
            false,
        ),
        (
            LocalDispatchState::HardStopRunningProvenNondispatch,
            Some(attempt),
            PersistentFailureStopEvidence::HardStopRunning,
            false,
        ),
        (
            LocalDispatchState::ProvenNondispatchSettling,
            Some(attempt),
            PersistentFailureStopEvidence::ProvenNondispatchSettling,
            false,
        ),
        (
            LocalDispatchState::PrimaryAccepted,
            Some(attempt),
            PersistentFailureStopEvidence::PrimaryAccepted,
            false,
        ),
        (
            LocalDispatchState::PossiblyDispatched,
            Some(attempt),
            PersistentFailureStopEvidence::PossiblyDispatched,
            false,
        ),
        (
            LocalDispatchState::DurablyAbandoned,
            Some(attempt),
            PersistentFailureStopEvidence::DurablyAbandoned,
            false,
        ),
        (
            LocalDispatchState::FailureFrozenNondispatch,
            Some(attempt),
            PersistentFailureStopEvidence::ClaimedNotDispatched,
            true,
        ),
    ];
    for (dispatch, retained_attempt, expected, permits_volatile) in cases {
        {
            let mut state = fixture.coordinator.state.lock().unwrap();
            let local = state.stops.get_mut(&fixture.thread).unwrap();
            local.dispatch = dispatch;
            local.attempt = retained_attempt;
        }
        let evidence = fixture
            .coordinator
            .persistent_failure_evidence(identity, fixture.thread)
            .unwrap();
        assert_eq!(evidence, expected);
        assert_eq!(evidence.permits_volatile_interrupt(), permits_volatile);
    }

    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
}

#[test]
fn stop_cancels_only_the_exact_automatic_phase_continuation() {
    let fixture = StopFixture::new(61);
    let other_turn = SyndicTurnId::from_bytes([0xee; 16]);
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                fixture.turn,
                LifecycleYieldOutcome::PhaseContinue,
            )
            .unwrap()
    );
    assert!(
        fixture
            .coordinator
            .record_lifecycle_yield(
                fixture.thread,
                other_turn,
                LifecycleYieldOutcome::PhaseNeedsReview,
            )
            .unwrap()
    );

    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first stop must own dispatch"),
        Err(error) => panic!("stop coordination failed: {error}"),
    };
    assert_eq!(
        fixture
            .coordinator
            .take_terminal_lifecycle_yield(fixture.thread, fixture.turn)
            .unwrap(),
        None
    );
    assert_eq!(
        fixture
            .coordinator
            .take_terminal_lifecycle_yield(fixture.thread, other_turn)
            .unwrap(),
        Some(LifecycleYieldOutcome::PhaseNeedsReview)
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));
}

#[test]
fn window_close_barrier_retains_exact_convergence_classification() {
    let fixture = StopFixture::new(71);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::HealthyHomeWindowClose,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        Ok(StopOwnership::Joined { .. }) => panic!("first close stop must own dispatch"),
        Err(error) => panic!("window-close stop coordination failed: {error}"),
    };
    let mut barrier = WindowCloseStopBarrier::new(
        Arc::clone(&fixture.coordinator),
        owner.operation_id,
        fixture.turn,
        true,
    );
    assert_eq!(barrier.operation_id(), owner.operation_id);
    assert!(barrier.primary_owner());
    assert_eq!(
        barrier.poll().unwrap(),
        WindowCloseStopBarrierStatus::Waiting
    );
    assert!(matches!(
        owner.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(_)
    ));

    let gate = InputGateRevision::new(1).unwrap();
    let matching_pending = StopAdmissionIneligibility::PendingTurn {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    let matching_terminal = StopAdmissionIneligibility::AwaitingTerminal {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    let matching_finalization = StopAdmissionIneligibility::FinalizingHistory {
        turn_id: fixture.turn,
        current_gate_revision: gate,
    };
    for reason in [matching_pending, matching_terminal, matching_finalization] {
        assert_eq!(
            window_close_ineligible_status(reason, fixture.turn).unwrap(),
            WindowCloseStopBarrierStatus::Waiting
        );
    }
    assert_eq!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::Idle {
                current_gate_revision: gate,
            },
            fixture.turn,
        )
        .unwrap(),
        WindowCloseStopBarrierStatus::Converged
    );
    assert_eq!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::Compacting {
                turn_id: other_turn(fixture.turn),
                current_gate_revision: gate,
            },
            fixture.turn,
        )
        .unwrap(),
        WindowCloseStopBarrierStatus::Converged
    );
    assert!(matches!(
        window_close_ineligible_status(
            StopAdmissionIneligibility::AwaitingSteering {
                turn_id: fixture.turn,
                current_gate_revision: gate,
            },
            fixture.turn,
        ),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
}

fn other_turn(turn: SyndicTurnId) -> SyndicTurnId {
    let mut bytes = *turn.as_bytes();
    bytes[0] ^= 0xff;
    SyndicTurnId::from_bytes(bytes)
}

fn published_activity(
    fixture: &StopFixture,
    seed: u8,
    kind: PublishedHardStopActivityKind,
    lifecycle: PublishedHardStopActivityLifecycle,
) -> PublishedHardStopActivity {
    PublishedHardStopActivity::new(
        fixture.thread,
        fixture.turn,
        fixture.target.loaded_generation(),
        fixture.target.cas_thread_id().clone(),
        fixture.target.cas_turn_id().clone(),
        SyndicItemId::from_bytes([seed; 16]),
        kind,
        lifecycle,
    )
}

fn attach_hard(
    fixture: &StopFixture,
    operation_id: StopOperationId,
) -> (hard::HardStopAttachment, Option<HardStopRunOwner>) {
    fixture
        .coordinator
        .attach_hard_stop(operation_id)
        .unwrap()
        .into_parts()
}

#[test]
fn provider_activity_publication_does_not_wait_for_stop_coordination_state() {
    let fixture = StopFixture::new(80);
    let effect = published_activity(
        &fixture,
        1,
        PublishedHardStopActivityKind::Command,
        PublishedHardStopActivityLifecycle::Active,
    );
    let state = fixture.coordinator.state.lock().unwrap();
    let coordinator = Arc::clone(&fixture.coordinator);
    let (published, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        coordinator.record_published_activity(effect);
        published.send(()).unwrap();
    });

    receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("provider activity must not acquire the stop-coordination mutex");
    drop(state);
    worker.join().unwrap();

    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), Some(HardStopTargetKind::CoarseThreadCleanup));
    run.finish_unavailable_without_dispatch().unwrap();
    assert_eq!(
        attachment.wait().unwrap().limitations()[1].omitted_active(),
        1
    );
}

#[test]
fn dropping_hard_stop_run_owner_preserves_durable_stop_without_home_io() {
    let fixture = StopFixture::new(81);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    let revision = fixture.home.home_revision().unwrap();

    drop(run);

    let _ = attachment.wait().unwrap();
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    let state = fixture.coordinator.state.lock().unwrap();
    let local = state.stops.get(&fixture.thread).unwrap();
    assert_eq!(local.operation_id, operation_id);
    assert_eq!(local.dispatch, LocalDispatchState::PossiblyDispatched);
    drop(state);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn direct_hard_cleanup_retains_original_election_until_authorization_boundary() {
    let fixture = StopFixture::new(96);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
    assert!(late_run.is_none());
    let mut run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached direct hard stop must reserve its run"),
    };
    assert!(!run.requires_fresh_election());

    let (wait_sender, wait_receiver) = mpsc::sync_channel(0);
    fixture
        .router
        .observe_next_terminal_publication_wait_for_test(wait_sender);
    let (terminal_sender, terminal_receiver) = mpsc::sync_channel(0);
    let router = Arc::clone(&fixture.router);
    let cas_thread_id = fixture.target.cas_thread_id().clone();
    let cas_turn_id = fixture.target.cas_turn_id().clone();
    let terminal = std::thread::spawn(move || {
        let published = router
            .acquire_terminal_source_publication(&cas_thread_id, &cas_turn_id)
            .is_ok_and(|permit| {
                permit
                    .finish_terminal(
                        crate::cas_projection::connection::ProvenTerminalOutcome::new(
                            TurnEndStatus::complete(),
                            timestamp(96),
                        ),
                    )
                    .is_ok()
            });
        terminal_sender.send(published).unwrap();
    });

    wait_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal publication must reach the inherited stop-election wait");
    assert!(matches!(
        terminal_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    run.release_inherited_election_after_authorization()
        .unwrap();
    assert!(
        terminal_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
    );
    terminal.join().unwrap();

    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);
    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(_)
    ));
    assert_eq!(attachment.wait().unwrap().targets().len(), 1);
}

#[test]
fn terminal_consumption_does_not_wait_behind_coordinate_blocked_on_router_publication() {
    let fixture = StopFixture::new(97);
    let terminal_permit = fixture
        .router
        .acquire_terminal_source_publication(
            fixture.target.cas_thread_id(),
            fixture.target.cas_turn_id(),
        )
        .unwrap();
    let (wait_sender, wait_receiver) = mpsc::sync_channel(0);
    fixture
        .router
        .observe_next_stop_election_wait_for_test(wait_sender);

    let (coordinate_sender, coordinate_receiver) = mpsc::sync_channel(0);
    let coordinator = Arc::clone(&fixture.coordinator);
    let router = Arc::clone(&fixture.router);
    let proof = fixture.proof.clone();
    let coordinate = std::thread::spawn(move || {
        coordinate_sender
            .send(coordinator.coordinate(&router, proof, StopCause::SelectedOperationControl))
            .unwrap();
    });
    wait_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("coordinate must reach the router wait behind terminal source publication");
    assert!(matches!(
        coordinate_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    let (consumed_sender, consumed_receiver) = mpsc::sync_channel(0);
    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let consumed = std::thread::spawn(move || {
        coordinator.terminal_consumed(thread_id, turn_id);
        consumed_sender.send(()).unwrap();
    });
    consumed_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("terminal consumption must not need state retained by router election waiting");

    terminal_permit
        .finish_terminal(
            crate::cas_projection::connection::ProvenTerminalOutcome::new(
                TurnEndStatus::complete(),
                timestamp(97),
            ),
        )
        .unwrap();
    assert!(
        coordinate_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .is_err()
    );
    consumed.join().unwrap();
    coordinate.join().unwrap();
}

#[test]
fn exact_terminal_and_authority_loss_clear_activity_without_a_local_stop() {
    for (seed, terminal) in [(84, true), (85, false)] {
        let fixture = StopFixture::new(seed);
        fixture
            .coordinator
            .record_published_activity(published_activity(
                &fixture,
                1,
                PublishedHardStopActivityKind::Command,
                PublishedHardStopActivityLifecycle::Active,
            ));
        if terminal {
            fixture
                .coordinator
                .terminal_consumed(fixture.thread, fixture.turn);
        } else {
            assert!(
                !fixture
                    .coordinator
                    .abandon_for_authority_loss(fixture.thread, fixture.turn)
                    .unwrap()
            );
        }

        let owner = match fixture.coordinator.coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
        ) {
            Ok(StopOwnership::Primary(owner)) => owner,
            _ => panic!("first hard stop must own primary dispatch"),
        };
        let (attachment, late_run) = attach_hard(&fixture, owner.operation_id());
        assert!(late_run.is_none());
        let run = match owner.settle_before_dispatch().unwrap() {
            StopDispatchSettlement::HardStop(run) => run,
            _ => panic!("attached hard stop must reserve its run"),
        };
        assert_eq!(run.target(), None);
        run.finish(None).unwrap();
        assert_eq!(
            attachment.wait().unwrap().limitations()[1].omitted_active(),
            0
        );
    }
}

#[test]
fn accepted_primary_admits_one_late_hard_run_and_duplicates_join_its_frozen_result() {
    let fixture = StopFixture::new(90);
    let mut primary = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    primary.begin_dispatch().unwrap();
    fixture
        .coordinator
        .mark_primary_accepted(fixture.thread, operation_id, primary.attempt)
        .unwrap();
    primary.settled = true;
    primary.permit.take().unwrap().finish();
    drop(primary);

    let (first, first_run) = attach_hard(&fixture, operation_id);
    let run = first_run.expect("first late attachment must own the hard continuation");
    let (second, second_run) = attach_hard(&fixture, operation_id);
    assert!(second_run.is_none(), "a duplicate must not own another run");
    assert_eq!(run.target(), None);
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));

    let (third, third_run) = attach_hard(&fixture, operation_id);
    assert!(
        third_run.is_none(),
        "a duplicate of a frozen result must remain attachment-only"
    );
    let first_result = first.wait().unwrap();
    assert_eq!(first_result, second.wait().unwrap());
    assert_eq!(first_result, third.wait().unwrap());
}

#[test]
fn completion_unknown_primary_cannot_admit_a_late_hard_run() {
    let fixture = StopFixture::new(91);
    let mut primary = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    primary.begin_dispatch().unwrap();
    fixture
        .coordinator
        .mark_possibly_dispatched(fixture.thread, operation_id, primary.attempt)
        .unwrap();
    primary.settled = true;
    primary.permit.take().unwrap().finish();
    drop(primary);

    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert_eq!(
        fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .stops
            .get(&fixture.thread)
            .unwrap()
            .dispatch,
        LocalDispatchState::PossiblyDispatched
    );
}

#[test]
fn terminal_or_consumed_hard_slot_cannot_be_recreated() {
    let terminal = StopFixture::new(92);
    let mut terminal_primary = match terminal.coordinator.coordinate(
        &terminal.router,
        terminal.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first terminal fixture stop must own primary dispatch"),
    };
    let terminal_operation = terminal_primary.operation_id();
    terminal
        .coordinator
        .terminal_consumed(terminal.thread, terminal.turn);
    terminal_primary.settled = true;
    terminal_primary.permit.take().unwrap().finish();
    drop(terminal_primary);
    assert!(matches!(
        terminal.coordinator.attach_hard_stop(terminal_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let consumed = StopFixture::new(93);
    let consumed_primary = match consumed.coordinator.coordinate(
        &consumed.router,
        consumed.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first consumed fixture stop must own primary dispatch"),
    };
    let consumed_operation = consumed_primary.operation_id();
    let (attachment, initial_run) = attach_hard(&consumed, consumed_operation);
    assert!(initial_run.is_none());
    consumed.coordinator.consume_hard_slot(consumed_operation);
    assert!(matches!(
        consumed.coordinator.attach_hard_stop(consumed_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    let run = match consumed_primary.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("the admitted attachment must retain its sole run"),
    };
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == consumed_operation
    ));
    assert_eq!(
        attachment.wait().unwrap().operation_id(),
        consumed_operation
    );
    assert!(matches!(
        consumed.coordinator.attach_hard_stop(consumed_operation),
        Err(StopCoordinationError::TargetUnavailable)
    ));
}

#[test]
fn proven_nondispatch_and_hard_attachment_have_closed_serialized_orders() {
    let attach_first = StopFixture::new(94);
    let primary = match attach_first.coordinator.coordinate(
        &attach_first.router,
        attach_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first attach-first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    let (attachment, premature_run) = attach_hard(&attach_first, operation_id);
    assert!(premature_run.is_none());
    let run = match primary.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attachment must linearize before nondispatch settlement"),
    };
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
    assert!(matches!(
        attach_first.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let settlement_first = StopFixture::new(95);
    let primary = match settlement_first.coordinator.coordinate(
        &settlement_first.router,
        settlement_first.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first settlement-first stop must own primary dispatch"),
    };
    let operation_id = primary.operation_id();
    assert!(matches!(
        primary.settle_before_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));
    assert!(matches!(
        settlement_first.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
    assert!(
        !settlement_first
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id),
        "settlement-first must not manufacture an unowned waiter slot"
    );
}

#[test]
fn duplicate_hard_callers_join_one_frozen_result_and_safe_reopen_waits_for_finish() {
    let fixture = StopFixture::new(81);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::ChildOrSubagent,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id;
    let (first, first_late_run) = attach_hard(&fixture, operation_id);
    let (second, second_late_run) = attach_hard(&fixture, operation_id);
    assert!(first_late_run.is_none());
    assert!(second_late_run.is_none());

    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            2,
            PublishedHardStopActivityKind::ChildOrSubagent,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), None);
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(_)
    ));
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));

    let first_result = first.wait().unwrap();
    let second_result = second.wait().unwrap();
    assert_eq!(first_result, second_result);
    assert!(first_result.targets().is_empty());
    assert_eq!(
        first_result.limitations()[0].limitation(),
        beryl_backend::ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported
    );
    assert_eq!(first_result.limitations()[0].omitted_active(), 1);
    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));
}

#[test]
fn pinned_command_snapshot_admits_only_one_coarse_cleanup_and_drop_settles_waiters() {
    let fixture = StopFixture::new(82);
    for seed in 0..=64 {
        fixture
            .coordinator
            .record_published_activity(published_activity(
                &fixture,
                seed,
                PublishedHardStopActivityKind::Command,
                PublishedHardStopActivityLifecycle::Active,
            ));
    }
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id;
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    assert_eq!(run.target(), Some(HardStopTargetKind::CoarseThreadCleanup));
    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::SafelyReopened(reopened) if reopened == operation_id
    ));

    let result = attachment.wait().unwrap();
    assert_eq!(result.targets().len(), 1);
    assert_eq!(
        result.targets()[0].disposition(),
        HardStopTargetDisposition::UnavailableWithoutDispatch
    );
    assert_eq!(result.limitations()[1].omitted_active(), 65);
    assert!(!result.limitations()[1].count_overflowed());
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Admissible(_)
    ));
}

#[test]
fn unavailable_cleanup_does_not_abandon_an_accepted_primary_stop() {
    let fixture = StopFixture::new(89);
    fixture
        .coordinator
        .record_published_activity(published_activity(
            &fixture,
            1,
            PublishedHardStopActivityKind::Command,
            PublishedHardStopActivityLifecycle::Active,
        ));
    let mut owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    owner.begin_dispatch().unwrap();
    let run = fixture
        .coordinator
        .begin_hard_run(
            operation_id,
            &owner.target,
            owner.attempt,
            true,
            owner.timeout,
        )
        .unwrap()
        .unwrap();
    owner.settled = true;
    owner.permit.take().unwrap().finish();
    drop(owner);

    assert!(matches!(
        run.finish_unavailable_without_dispatch().unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    let result = attachment.wait().unwrap();
    assert_eq!(
        result.targets()[0].disposition(),
        HardStopTargetDisposition::UnavailableWithoutDispatch
    );
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(live) if live.operation_id() == operation_id
    ));
}

#[test]
fn terminal_consumption_holds_only_finalization_release_until_hard_run_finishes() {
    let fixture = StopFixture::new(83);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id;
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);
    assert!(matches!(
        fixture.coordinator.attach_hard_stop(operation_id),
        Err(StopCoordinationError::TargetUnavailable)
    ));

    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let (released, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        coordinator
            .wait_for_finalization_release(thread_id, turn_id)
            .unwrap();
        released.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());
    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
}

#[test]
fn consumed_finished_slot_without_waiters_is_removed_immediately() {
    let fixture = StopFixture::new(86);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    drop(attachment);
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    fixture
        .coordinator
        .terminal_consumed(fixture.thread, fixture.turn);

    assert!(matches!(
        run.finish(None).unwrap(),
        StopDispatchSettlement::Stopping(stopping) if stopping == operation_id
    ));
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );

    let fixture = StopFixture::new(88);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    drop(attachment);
    fixture.coordinator.consume_hard_slot(operation_id);
    fixture
        .coordinator
        .finish_hard_without_run(operation_id)
        .unwrap();
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );
    drop(owner);
}

#[test]
fn durable_settlement_error_still_finishes_result_and_releases_hold() {
    let fixture = StopFixture::new(87);
    let owner = match fixture.coordinator.coordinate(
        &fixture.router,
        fixture.proof.clone(),
        StopCause::SelectedOperationControl,
    ) {
        Ok(StopOwnership::Primary(owner)) => owner,
        _ => panic!("first hard stop must own primary dispatch"),
    };
    let operation_id = owner.operation_id();
    let (attachment, late_run) = attach_hard(&fixture, operation_id);
    assert!(late_run.is_none());
    let run = match owner.settle_before_dispatch().unwrap() {
        StopDispatchSettlement::HardStop(run) => run,
        _ => panic!("attached hard stop must reserve its run"),
    };
    fixture.coordinator.remove_local(operation_id);
    fixture.coordinator.consume_hard_slot(operation_id);

    let coordinator = Arc::clone(&fixture.coordinator);
    let thread_id = fixture.thread;
    let turn_id = fixture.turn;
    let (released, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        coordinator
            .wait_for_finalization_release(thread_id, turn_id)
            .unwrap();
        released.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());

    assert!(matches!(
        run.finish(None),
        Err(StopCoordinationError::LocalAuthorityMismatch)
    ));
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
    assert_eq!(attachment.wait().unwrap().operation_id(), operation_id);
    assert!(
        !fixture
            .coordinator
            .state
            .lock()
            .unwrap()
            .hard
            .slots
            .contains_key(&operation_id)
    );
}
