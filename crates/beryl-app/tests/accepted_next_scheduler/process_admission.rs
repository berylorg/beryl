use super::*;
use beryl_app::{
    cas_projection::{
        ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
        ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
        test_faults::ProjectionConnectionRetirementHandle,
    },
    process_admission::ProcessAdmissionError,
};

fn prepared_fixture(seed: u8) -> (syndic::Fixture, SessionSlot) {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(seed, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text(" process fence parent");
    fixture.complete_with_assistant(parent, " process fence answer");
    (fixture, slot)
}

fn connect(
    fixture: &syndic::Fixture,
    slot: &SessionSlot,
    server: &NormalTerminalServer,
    generation: u64,
) -> ProjectionConnectionRetirementHandle {
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    server.wait_for_admission();
    retirement
}

fn assert_unpromoted(fixture: &syndic::Fixture, ids: &support::NextRecordIds) {
    let command = fixture.store.live_home_command().unwrap();
    assert!(matches!(
        accepted_route_state(command.home(), &fixture.storage, ids),
        AcceptedRouteEffectiveState::NextTurn(_)
    ));
    assert_eq!(
        fixture
            .storage
            .thread(command.home(), ids.thread, support::point_limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(ids.parent)
    );
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());
}

fn wait_for_worker(fixture: &syndic::Fixture, slot: &SessionSlot) {
    wait_until("fenced promotion worker settlement", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_joined() >= 1 && diagnostics.workers_active() == 0 && slot.is_ready())
            .then_some(())
    });
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());
}

fn finish(fixture: syndic::Fixture, slot: SessionSlot, server: NormalTerminalServer) {
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn process_fence_before_reservation_preserves_the_same_accepted_candidate() {
    let (mut fixture, slot) = prepared_fixture(211);
    let server = NormalTerminalServer::spawn_admission_only();
    let retirement = connect(&fixture, &slot, &server, 62_031);
    let barrier = install_scheduled_promotion_reservation_barrier(fixture.thread);
    let ids = admit_runtime_next_input(&mut fixture, 211);
    assert!(barrier.wait_until_paused(TIMEOUT));
    let fence = fixture.process_admission.test_fence().unwrap();
    assert_unpromoted(&fixture, &ids);
    barrier.release();
    wait_for_worker(&fixture, &slot);
    assert_unpromoted(&fixture, &ids);
    let command = fixture.store.live_home_command().unwrap();
    fence
        .try_reopen(command.home().pending_reconciliations().is_empty())
        .unwrap();
    drop(command);
    drop(retirement);
    finish(fixture, slot, server);
}

#[test]
fn process_fence_waits_for_a_winning_promotion_reservation_to_release() {
    let (mut fixture, slot) = prepared_fixture(212);
    let server = NormalTerminalServer::spawn_admission_only();
    let retirement = connect(&fixture, &slot, &server, 62_032);
    let barrier = install_scheduled_promotion_barrier(fixture.thread);
    let ids = admit_runtime_next_input(&mut fixture, 212);
    assert!(barrier.wait_until_paused(TIMEOUT));
    let fence = fixture.process_admission.test_fence().unwrap();
    assert_eq!(
        fence.try_reopen(true),
        Err(ProcessAdmissionError::Unsettled)
    );
    let retirement_worker = {
        let retirement = retirement.clone();
        thread::spawn(move || retirement.retire())
    };
    wait_until("winning promotion connection retirement", || {
        retirement.is_retired().then_some(())
    });
    assert!(!retirement_worker.is_finished());
    barrier.release();
    retirement_worker.join().unwrap();
    wait_for_worker(&fixture, &slot);
    let command = fixture.store.live_home_command().unwrap();
    assert_eq!(
        accepted_route_state(command.home(), &fixture.storage, &ids),
        AcceptedRouteEffectiveState::Promoted
    );
    fence
        .try_reopen(command.home().pending_reconciliations().is_empty())
        .unwrap();
    drop(command);
    drop(retirement);
    finish(fixture, slot, server);
}

#[test]
fn indeterminate_promotion_retains_process_custody_through_worker_reconciliation() {
    use beryl_app::cas_projection::test_faults::install_scheduled_promotion_reconciliation_barrier;
    use beryl_home_store::test_faults::FaultPoint;

    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_and_faults(
        214,
        faults.clone(),
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text(" reconciliation fence parent");
    fixture.complete_with_assistant(parent, " reconciliation fence answer");
    let server = NormalTerminalServer::spawn_admission_only();
    let retirement = connect(&fixture, &slot, &server, 62_034);
    let reserved = install_scheduled_promotion_barrier(fixture.thread);
    let outcome = install_scheduled_promotion_reconciliation_barrier(fixture.thread);
    let ids = admit_runtime_next_input(&mut fixture, 214);
    assert!(reserved.wait_until_paused(TIMEOUT));
    let fence = fixture.process_admission.test_fence().unwrap();
    let command = fixture.store.live_home_command().unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    reserved.release();
    assert!(outcome.wait_until_paused(TIMEOUT));
    assert!(command.home().pending_reconciliations().is_empty());
    assert_eq!(
        fence.try_reopen(true),
        Err(ProcessAdmissionError::Unsettled)
    );
    let retirement_worker = {
        let retirement = retirement.clone();
        thread::spawn(move || retirement.retire())
    };
    wait_until("indeterminate promotion connection retirement", || {
        retirement.is_retired().then_some(())
    });
    assert!(!retirement_worker.is_finished());
    outcome.release();
    retirement_worker.join().unwrap();
    wait_for_worker(&fixture, &slot);
    assert_eq!(
        accepted_route_state(command.home(), &fixture.storage, &ids),
        AcceptedRouteEffectiveState::Promoted
    );
    assert!(command.home().pending_reconciliations().is_empty());
    fence.try_reopen(true).unwrap();
    drop(command);
    drop(retirement);
    finish(fixture, slot, server);
}

#[test]
fn connection_retirement_does_not_hide_a_failed_reserved_promotion() {
    use beryl_app::cas_projection::test_faults::install_scheduled_promotion_reconciliation_barrier;
    use beryl_home_store::test_faults::FaultPoint;

    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_and_faults(
        215,
        faults.clone(),
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text(" failed promotion parent");
    fixture.complete_with_assistant(parent, " failed promotion answer");
    let server = NormalTerminalServer::spawn_admission_only();
    let retirement = connect(&fixture, &slot, &server, 62_035);
    let reserved = install_scheduled_promotion_barrier(fixture.thread);
    let outcome = install_scheduled_promotion_reconciliation_barrier(fixture.thread);
    admit_runtime_next_input(&mut fixture, 215);
    assert!(reserved.wait_until_paused(TIMEOUT));
    let _fence = fixture.process_admission.test_fence().unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    reserved.release();
    assert!(outcome.wait_until_paused(TIMEOUT));
    let retirement_worker = {
        let retirement = retirement.clone();
        thread::spawn(move || retirement.retire())
    };
    wait_until("failed promotion connection retirement", || {
        retirement.is_retired().then_some(())
    });
    assert!(!retirement_worker.is_finished());
    outcome.release();
    retirement_worker.join().unwrap();
    wait_until("failed promotion scheduler classification", || {
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .fatal()
            .then_some(())
    });
    drop(retirement);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

struct PausingProvider {
    inner: Box<dyn ScheduledOrdinaryExecutionProvider>,
    entered: Option<std::sync::mpsc::SyncSender<()>>,
    release: std::sync::mpsc::Receiver<()>,
}

impl ScheduledOrdinaryExecutionProvider for PausingProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        let result = self.inner.try_issue(admission)?;
        if matches!(result, ScheduledOrdinaryAdmissionResult::Issued(_))
            && let Some(entered) = self.entered.take()
        {
            entered.send(()).unwrap();
            self.release.recv_timeout(TIMEOUT).unwrap();
        }
        Ok(result)
    }

    fn shutdown(&mut self) {
        self.inner.shutdown();
    }
}

#[test]
fn provider_preparation_cannot_refresh_a_candidate_after_process_reopening() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let (entered_tx, entered) = sync_channel(1);
    let (release, release_rx) = sync_channel(1);
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(213, move |assets| {
        Box::new(PausingProvider {
            inner: Box::new(ready_provider(provider_slot, assets)),
            entered: Some(entered_tx),
            release: release_rx,
        })
    });
    let parent = fixture.submit_text(" delayed provider parent");
    fixture.complete_with_assistant(parent, " delayed provider answer");
    let server = NormalTerminalServer::spawn_admission_only();
    let retirement = connect(&fixture, &slot, &server, 62_033);
    let ids = admit_runtime_next_input(&mut fixture, 213);
    entered.recv_timeout(TIMEOUT).unwrap();
    let fence = fixture.process_admission.test_fence().unwrap();
    fence.try_reopen(true).unwrap();
    release.send(()).unwrap();
    wait_for_worker(&fixture, &slot);
    assert_unpromoted(&fixture, &ids);
    drop(retirement);
    finish(fixture, slot, server);
}
