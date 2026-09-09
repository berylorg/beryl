use std::path::Path;

use beryl_app::cas_projection::*;
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};
use beryl_model::{CasProcessGeneration, CasThreadId};
use syndic_storage::SyndicTimestamp;

use super::{
    EXECUTION_ROOT, NoopBranch, NoopLifecycle, TEST_LOCK,
    server::{AUTHORIZATION, CAS_THREAD_ID, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT},
    stop_retention::wait_for_active,
    syndic::{Fixture, execution_binding},
};

fn page(fixture: &Fixture) -> StopWorkPage {
    let revision = fixture.store.stop_work_revision().unwrap();
    let page = fixture
        .store
        .stop_work_page(
            &revision,
            None,
            StopWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    let control_revision = fixture.store.control_work_revision().unwrap();
    let control = fixture
        .store
        .control_work_page(
            &control_revision,
            None,
            ControlWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(control.stop_records(), page.records());
    assert!(control.compaction_records().is_empty());
    page
}

fn permission(page: &StopWorkPage) -> &PermissionInterruptionWorkFact {
    page.records()
        .iter()
        .find_map(|record| match record {
            StopWorkRecord::Permission(fact) => Some(fact),
            _ => None,
        })
        .expect("exact permission obligation remains observed")
}

#[test]
fn permission_work_survives_preparation_driver_cleanup_and_target_loss() {
    let _lock = TEST_LOCK.lock().unwrap();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider_faults_and_capacity(
        161,
        beryl_home_store::test_faults::FaultController::new(),
        4,
        move |_| Box::new(provider),
    );
    let attention = beryl_app::lifecycle_attention::ProcessLifecycleAttentionPool::new();
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_permission_stop();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_161).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
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
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    std::thread::scope(|scope| {
        let fixture = &fixture;
        let capture = scope.spawn(move || {
            coordinator.execute_ordinary_turn(
                &*fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                OrdinaryDynamicToolHandlers::new(
                    &mut NoopLifecycle::default(),
                    &mut NoopBranch::default(),
                ),
            )
        });
        wait_for_active(fixture, submitted.turn);
        let install =
            test_faults::install_approval_install_barrier(CasThreadId::new(CAS_THREAD_ID).unwrap());
        let cleanup = test_faults::install_stop_cleanup_barrier(fixture.thread);
        server.send_permission();
        if !install.wait_for_route_timeout(TIMEOUT) {
            panic!(
                "permission did not reach prepared installation; capture: {:?}",
                capture.join().unwrap()
            );
        }
        let prepared = page(fixture);
        let inventory = fixture.store.process_work_inventory(&sessions, &attention);
        let inventory_page = || {
            let revision = inventory.revision().unwrap();
            inventory
                .page(
                    &revision,
                    None,
                    ProcessWorkPageLimits::new(256, 65_536).unwrap(),
                    &ProjectionCancellationToken::new(),
                )
                .unwrap()
        };
        let fact = permission(&prepared);
        assert_eq!(fact.stage, PermissionInterruptionWorkStage::Prepared);
        assert_eq!(fact.thread_id, fixture.thread);
        let serial = fact.serial;
        let operation = fact.operation_id.unwrap();
        assert_eq!(prepared.records().len(), 2);
        assert_eq!(prepared, page(fixture));
        let first = fixture
            .store
            .stop_work_page(
                prepared.revision(),
                None,
                StopWorkPageLimits::new(1, 65_536).unwrap(),
            )
            .unwrap();
        let second = fixture
            .store
            .stop_work_page(
                prepared.revision(),
                first.next_cursor(),
                StopWorkPageLimits::new(1, 65_536).unwrap(),
            )
            .unwrap();
        assert!(matches!(first.records(), [StopWorkRecord::Stop(_)]));
        assert!(matches!(second.records(), [StopWorkRecord::Permission(_)]));
        assert!(second.next_cursor().is_none());
        install.release();
        cleanup.wait();
        assert_eq!(
            fixture
                .store
                .validate_stop_work_revision(prepared.revision()),
            Err(StopWorkError::StaleRevision)
        );
        let driver = page(fixture);
        let running = inventory_page();
        assert_eq!(running.total_threads(), 1);
        assert!(running.records()[0].facts.stopping && running.records()[0].facts.request_handling);
        let fact = permission(&driver);
        assert_eq!(fact.stage, PermissionInterruptionWorkStage::Driver);
        assert_eq!((fact.serial, fact.operation_id), (serial, Some(operation)));
        let abandonment =
            test_faults::install_live_event_target_abandonment(&session, fixture.thread);
        assert!(abandonment.wait_until_abandoned(TIMEOUT));
        assert!(matches!(
            capture.join().unwrap().unwrap(),
            OrdinaryTurnExecutionOutcome::Incomplete { .. }
        ));
        let lost = page(fixture);
        let running = inventory_page();
        assert_eq!(running.total_threads(), 1);
        assert!(running.records()[0].facts.stopping);
        assert_eq!(permission(&lost).serial, serial);
        let StopWorkRecord::Stop(stop) = &lost.records()[0] else {
            panic!("driver stop cleanup remains");
        };
        assert!(stop.local_dispatch.is_none() && stop.driver_custody && !stop.primary_custody);
        cleanup.release(false);
        session.invalidate_connection();
        assert!(page(fixture).records().is_empty());
        assert_eq!(
            fixture.store.stop_work_page(
                prepared.revision(),
                first.next_cursor(),
                StopWorkPageLimits::new(1, 65_536).unwrap()
            ),
            Err(StopWorkError::StaleRevision)
        );
    });
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    drop(directory);
}
