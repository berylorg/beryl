use std::{
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasProcessGeneration, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicThreadId,
};
use beryl_state::{AssetState, BerylState};
use syndic_storage::SyndicStorage;

use crate::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        OrdinaryDynamicToolContext, ProjectionConnectionService,
        ProjectionConnectionServiceCloseOutcome, ProjectionCoordinatorError,
        ProjectionServiceConfig, service_config::ProjectionWorkerPermitError,
    },
};

use super::*;

mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
    ));
}

use server::{AUTHORIZATION, NormalTerminalServer, TIMEOUT};

struct ReturningSession {
    session: Option<AdmittedProjectionSession>,
    slot: Arc<Mutex<Option<AdmittedProjectionSession>>>,
}

impl ScheduledProjectionSessionAuthority for ReturningSession {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.session
            .as_mut()
            .expect("checked-out scheduled session remains owned")
    }
}

impl Drop for ReturningSession {
    fn drop(&mut self) {
        let session = self
            .session
            .take()
            .expect("checked-out scheduled session returns exactly once");
        let mut slot = self.slot.lock().unwrap();
        assert!(slot.replace(session).is_none());
    }
}

struct LifecycleHandler;

impl LifecycleYieldRequestHandler for LifecycleHandler {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("lifecycle")
    }
}

struct BranchHandler;

impl BranchDiscussionResolutionRequestHandler for BranchHandler {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("branch")
    }
}

struct ToolAuthority {
    lifecycle: LifecycleHandler,
    branch: BranchHandler,
}

impl OrdinaryDynamicToolAuthority for ToolAuthority {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        OrdinaryDynamicToolHandlers::new(&mut self.lifecycle, &mut self.branch)
    }
}

struct CheckoutProvider {
    slot: Arc<Mutex<Option<AdmittedProjectionSession>>>,
    policy: ScheduledOrdinaryRequestPolicy,
    assets: Arc<Mutex<AssetState>>,
}

impl ScheduledOrdinaryExecutionProvider for CheckoutProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let Some(session) = self.slot.lock().unwrap().take() else {
            return Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::SessionBusy));
        };
        let session = ReturningSession {
            session: Some(session),
            slot: Arc::clone(&self.slot),
        };
        admission
            .issue(
                Box::new(session),
                self.policy.clone(),
                *self.assets.lock().unwrap(),
                Box::new(ToolAuthority {
                    lifecycle: LifecycleHandler,
                    branch: BranchHandler,
                }),
            )
            .map(ScheduledOrdinaryAdmissionResult::Issued)
    }

    fn shutdown(&mut self) {
        self.slot.lock().unwrap().take();
    }
}

fn execution_binding(runtime_id: RuntimeId) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([92; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\phase59-scheduled-ordinary",
        )
        .unwrap(),
    )
}

fn explicit_policy() -> ScheduledOrdinaryRequestPolicy {
    let turn = TurnStartOptions::default()
        .with_model("turn-model")
        .with_reasoning_effort("high")
        .with_developer_instructions_context(
            Some("phase59 developer instructions".to_owned()),
            "turn-model",
            Some("high".to_owned()),
        );
    ScheduledOrdinaryRequestPolicy::new(
        ThreadStartOptions::persistent().with_model("thread-model"),
        Some(65_536),
        Duration::from_secs(29),
        OrdinaryTurnExecutionRequest::new(turn, Duration::from_secs(31)),
    )
}

fn wait_for_worker_availability(service: &ProjectionConnectionService, expected: usize) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let diagnostics = service.worker_pool_diagnostics();
        if diagnostics.available() == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "worker availability did not settle at {expected}: {diagnostics:?}"
        );
        thread::yield_now();
    }
}

#[test]
fn request_policy_preserves_every_explicit_late_bound_input() {
    let thread = ThreadStartOptions::persistent().with_model("thread-model");
    let turn_options = TurnStartOptions::default()
        .with_model("turn-model")
        .with_reasoning_effort("high");
    let turn = OrdinaryTurnExecutionRequest::new(turn_options, Duration::from_secs(31));
    let policy = ScheduledOrdinaryRequestPolicy::new(
        thread.clone(),
        Some(65_536),
        Duration::from_secs(29),
        turn.clone(),
    );

    assert_eq!(policy.thread_options(), &thread);
    assert_eq!(policy.model_context_window_tokens(), Some(65_536));
    assert_eq!(policy.projection_timeout(), Duration::from_secs(29));
    assert_eq!(policy.turn(), &turn);
}

#[test]
fn exact_lease_protects_steering_and_returns_session_and_flight() {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let slot = Arc::new(Mutex::new(None));
    let provider_assets = Arc::new(Mutex::new(state.assets()));
    let provider = CheckoutProvider {
        slot: Arc::clone(&slot),
        policy: explicit_policy(),
        assets: Arc::clone(&provider_assets),
    };
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4).unwrap(),
        Box::new(provider),
    )
    .unwrap();
    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let runtime_id = RuntimeId::from_bytes([91; 16]);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(59_001).unwrap(),
            Path::new(r"C:\work\phase59-scheduled-ordinary"),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    *slot.lock().unwrap() = Some(session);

    let thread_id = SyndicThreadId::from_bytes([93; 16]);
    let foreign_directory = tempfile::tempdir().unwrap();
    let mut foreign_home = HomeStore::open(HomeOpenOptions::new(
        foreign_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let foreign_storage = SyndicStorage::register(&mut foreign_home).unwrap();
    let foreign_state = BerylState::register(&mut foreign_home).unwrap();
    let foreign_assets = foreign_state.assets();
    let foreign_slot = Arc::new(Mutex::new(None));
    let foreign_provider = CheckoutProvider {
        slot: Arc::clone(&foreign_slot),
        policy: explicit_policy(),
        assets: Arc::new(Mutex::new(foreign_assets)),
    };
    let foreign_service = ProjectionConnectionService::new(
        foreign_home,
        foreign_storage,
        ProjectionServiceConfig::try_new(8, 4).unwrap(),
        Box::new(foreign_provider),
    )
    .unwrap();
    let foreign_server = NormalTerminalServer::spawn_admission_only();
    let foreign_connector =
        ManagedBackendClientConnector::for_lifecycle_test(foreign_server.endpoint(), AUTHORIZATION);
    let foreign_session = foreign_service
        .admit_lifecycle_test_candidate(
            &foreign_connector,
            runtime_id,
            CasProcessGeneration::new(59_002).unwrap(),
            Path::new(r"C:\work\phase59-scheduled-ordinary"),
            TIMEOUT,
        )
        .unwrap();
    foreign_server.wait_for_admission();

    let owned_session = slot.lock().unwrap().take().unwrap();
    assert!(slot.lock().unwrap().replace(foreign_session).is_none());
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let foreign = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(runtime_id),
            worker,
            flight,
        )
        .unwrap_err();
    assert!(matches!(
        foreign,
        ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable {
            runtime_id: actual_runtime,
            process_generation,
        } if actual_runtime == runtime_id
            && process_generation == CasProcessGeneration::new(59_002).unwrap()
    ));
    wait_for_worker_availability(&service, 2);
    drop(slot.lock().unwrap().take().unwrap());
    assert!(slot.lock().unwrap().replace(owned_session).is_none());
    assert!(matches!(
        foreign_service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    foreign_server.join();
    drop(foreign_slot);
    drop(foreign_directory);

    *provider_assets.lock().unwrap() = foreign_assets;
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let foreign_assets_error = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(runtime_id),
            worker,
            flight,
        )
        .unwrap_err();
    assert!(matches!(
        foreign_assets_error,
        ScheduledOrdinaryAdmissionError::AssetAuthority { .. }
    ));
    assert!(slot.lock().unwrap().is_some());
    wait_for_worker_availability(&service, 2);
    *provider_assets.lock().unwrap() = state.assets();

    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let mismatch = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(RuntimeId::from_bytes([94; 16])),
            worker,
            flight,
        )
        .unwrap_err();
    assert!(matches!(
        mismatch,
        ScheduledOrdinaryAdmissionError::RuntimeMismatch {
            requested,
            admitted,
        } if requested == RuntimeId::from_bytes([94; 16]) && admitted == runtime_id
    ));
    assert!(slot.lock().unwrap().is_some());
    wait_for_worker_availability(&service, 2);

    let held_session = slot.lock().unwrap().take().unwrap();
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let unavailable = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(runtime_id),
            worker,
            flight,
        )
        .unwrap();
    assert!(matches!(
        unavailable,
        ScheduledOrdinaryAdmissionResult::Unavailable(
            ScheduledOrdinaryExecutionUnavailable::SessionBusy
        )
    ));
    wait_for_worker_availability(&service, 2);
    drop(service.begin_scheduled_ordinary_flight(thread_id).unwrap());
    assert!(slot.lock().unwrap().replace(held_session).is_none());

    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let result = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(runtime_id),
            worker,
            flight,
        )
        .unwrap();
    let ScheduledOrdinaryAdmissionResult::Issued(lease) = result else {
        panic!("ready provider declined exact execution authority");
    };
    assert_eq!(lease.home_id(), service.home_id());
    assert_eq!(lease.home_generation(), service.home_generation());
    assert_eq!(lease.thread_id(), thread_id);
    assert_eq!(lease.execution_binding(), &execution_binding(runtime_id));
    assert_eq!(
        lease.process_generation(),
        CasProcessGeneration::new(59_001).unwrap()
    );
    {
        let live_home = service.live_home_command().unwrap();
        assert_eq!(
            lease.assets().revision(live_home.home()).unwrap(),
            state.assets().revision(live_home.home()).unwrap()
        );
    }
    assert_eq!(
        lease.policy().turn().start_options().model(),
        Some("turn-model")
    );
    assert_eq!(
        lease.policy().turn().start_options().reasoning_effort(),
        Some("high")
    );
    let developer = lease
        .policy()
        .turn()
        .start_options()
        .developer_instructions_context()
        .unwrap();
    assert_eq!(
        developer.developer_instructions(),
        Some("phase59 developer instructions")
    );
    assert!(slot.lock().unwrap().is_none());

    assert!(matches!(
        service.try_acquire_scheduled_ordinary_worker(),
        Err(ProjectionWorkerPermitError::CapacityFull { available: 1 })
    ));
    let steering = service.acquire_steering_worker_for_test().unwrap();
    assert_eq!(service.worker_pool_diagnostics().available(), 0);
    drop(steering);

    let diagnostics_before = service.accepted_input_scheduler_diagnostics();
    assert!(matches!(
        service.begin_scheduled_ordinary_flight(thread_id),
        Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id: actual })
            if actual == thread_id
    ));
    drop(lease);
    let diagnostics_after = service.accepted_input_scheduler_diagnostics();
    let new_wakes = diagnostics_after.wake_count() - diagnostics_before.wake_count();
    let newly_coalesced =
        diagnostics_after.coalesced_wake_count() - diagnostics_before.coalesced_wake_count();
    assert!(new_wakes >= 1);
    assert_eq!(
        new_wakes + newly_coalesced,
        1,
        "the earlier steering release consumed the armed capacity waiter, so lease release only wakes the retained same-thread flight waiter"
    );
    assert!(slot.lock().unwrap().is_some());
    wait_for_worker_availability(&service, 2);

    let retired_session = slot.lock().unwrap().take().unwrap();
    retired_session.invalidate_connection();
    assert!(slot.lock().unwrap().replace(retired_session).is_none());
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread_id).unwrap();
    let retired = service
        .issue_scheduled_ordinary_execution(
            thread_id,
            execution_binding(runtime_id),
            worker,
            flight,
        )
        .unwrap_err();
    assert!(matches!(
        retired,
        ScheduledOrdinaryAdmissionError::SessionAuthorityUnavailable { .. }
    ));
    assert!(slot.lock().unwrap().is_some());
    wait_for_worker_availability(&service, 4);

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert!(slot.lock().unwrap().is_none());
    drop(slot);
    drop(directory);
}
