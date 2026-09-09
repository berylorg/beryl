use super::*;
use beryl_app::{
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        AdmittedProjectionSession, OrdinaryDynamicToolContext, ProcessLifecycleYieldHandler,
    },
};
use beryl_backend::{DynamicToolCallResponse, ManagedBackendClientConnector};
use beryl_model::CasProcessGeneration;
use server::{AUTHORIZATION, NormalTerminalServer, TIMEOUT};
use std::{path::Path, sync::mpsc};

struct PausedAcceptance {
    inner: ProcessLifecycleYieldHandler,
    accepted: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}

impl LifecycleYieldRequestHandler for PausedAcceptance {
    fn respond_lifecycle_yield(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        let response = self.inner.respond_lifecycle_yield(context, request);
        assert!(response.success);
        self.accepted.send(()).unwrap();
        self.release.recv_timeout(TIMEOUT).unwrap();
        response
    }
}

struct CaptureRelease<'a> {
    session: &'a AdmittedProjectionSession,
    release: mpsc::SyncSender<()>,
}

impl Drop for CaptureRelease<'_> {
    fn drop(&mut self) {
        self.session.invalidate_connection();
        let _ = self.release.try_send(());
    }
}

#[test]
fn direct_continuation_acceptance_retains_capacity_after_connection_reuse() {
    run_paused_acceptance(false);
}

#[test]
fn cancelled_direct_continuation_retains_capacity_until_caller_disposal() {
    run_paused_acceptance(true);
}

fn run_paused_acceptance(cancelled: bool) {
    let mut fixture = Fixture::new_with_worker_capacity(240, 4);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let custody = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pressure = custody.occupy_compaction_custody(71);
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let mut lifecycle = PausedAcceptance {
        inner: fixture.store.lifecycle_yield_handler(&pool),
        accepted: accepted_tx,
        release: release_rx,
    };
    thread::scope(|scope| {
        let release = CaptureRelease {
            session: &session,
            release: release_tx,
        };
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        server.call_until_connection_close("phase_continue");
        accepted_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(pressure.in_use(), 72);
        assert!(pool.snapshot().is_empty());
        if cancelled {
            fixture
                .store
                .cancel_selected_continuation_for_window_close(fixture.thread)
                .unwrap();
            assert_eq!(pressure.in_use(), 72);
        }
        session.invalidate_connection();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 0);
        assert_eq!(pressure.in_use(), 72);
        let replacement_server = NormalTerminalServer::spawn_admission_only();
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            replacement_server.endpoint(),
            AUTHORIZATION,
        );
        let replacement = fixture
            .store
            .admit_lifecycle_test_candidate(
                &connector,
                syndic::execution_binding().runtime_id(),
                CasProcessGeneration::new(970_002).unwrap(),
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        replacement_server.wait_for_admission();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
        assert_eq!(pressure.in_use(), 72);
        replacement.invalidate_connection();
        drop(replacement);
        replacement_server.join();
        drop(release);
        let _result = worker.join().unwrap();
        assert_eq!(pressure.in_use(), 71);
        let replacement_slot = custody.occupy_compaction_custody(1);
        assert_eq!(pressure.in_use(), 72);
        drop(replacement_slot);
    });
    assert_eq!(pool.snapshot().len(), usize::from(!cancelled));
    drop(pressure);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn continuation_capacity_denial_creates_no_attention_and_preserves_other_yields() {
    let mut fixture = Fixture::new(241);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let custody = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pressure = custody.occupy_compaction_custody(72);
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("phase_continue")["result"]["success"], false);
        assert!(pool.snapshot().is_empty());
        assert_eq!(pressure.in_use(), 72);
        assert_eq!(server.call("plan_complete")["result"]["success"], true);
        server.finish(false);
        worker.join().unwrap().unwrap()
    });
    assert!(matches!(
        result,
        OrdinaryTurnExecutionOutcome::Terminal { .. }
    ));
    assert_eq!(pool.snapshot().len(), 1);
    assert_eq!(
        pool.snapshot()[0].kind(),
        LifecycleAttentionKind::PlanComplete
    );
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    assert_eq!(pressure.in_use(), 72);
    drop(pressure);
    drop(directory);
}
