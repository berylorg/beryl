use super::*;
use beryl_app::cas_projection::{
    CompactionCustodyTestStage, ContextCompactionError, ContextCompactionRequest,
};
use syndic_storage::CompactionAdmissionRead;

#[test]
fn failed_compaction_caller_retains_capacity_after_connection_reuse() {
    run(false, false);
}

#[test]
fn failed_compaction_caller_releases_capacity_on_unwind() {
    run(true, false);
}

#[test]
fn admitted_compaction_unwind_releases_registry_owned_custody() {
    run(true, true);
}

fn run(unwind: bool, before_registration: bool) {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new_with_worker_capacity(159, 4);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_159).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &beryl_app::cas_projection::CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                syndic_storage::SyndicTimestamp::from_unix_millis(37_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let outcome = coordinator
        .execute_ordinary_turn(
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
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, .. } = outcome else {
        panic!("expected completed ordinary projection");
    };
    let harness = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pressure = harness.occupy_compaction_custody(71);
    std::thread::scope(|scope| {
        let ready = harness.pause_compaction_custody(CompactionCustodyTestStage::AdmissionReady);
        let failed = harness.pause_compaction_custody(CompactionCustodyTestStage::AdmissionFailed);
        let fixture = &fixture;
        let caller = scope.spawn(|| {
            fixture
                .store
                .compact_thread(ContextCompactionRequest::new(fixture.thread, TIMEOUT))
        });
        ready.wait_until_paused();
        assert_eq!(harness.compaction_custody_in_use(), 72);
        assert!(harness.has_local_compaction(fixture.thread));
        let admitted_page = work_page(fixture);
        assert_eq!(admitted_page.records().len(), 1);
        let admitted = admitted_page.records()[0].compaction.as_ref().unwrap();
        assert!(admitted.local_registered);
        assert!(admitted.command.is_some());
        assert_eq!(
            admitted
                .operation
                .as_ref()
                .unwrap()
                .operation_id
                .thread_id(),
            fixture.thread
        );
        assert_eq!(
            fixture.store.compaction_work_revision().unwrap(),
            *admitted_page.revision()
        );
        let result_waiter = harness.retain_compaction_result(fixture.thread);
        session.invalidate_connection();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 0);
        if before_registration {
            ready.unwind();
            assert!(caller.join().is_err());
            assert!(!harness.has_local_compaction(fixture.thread));
            assert_eq!(harness.compaction_custody_in_use(), 71);
            assert!(work_page(fixture).records().is_empty());
            assert_eq!(admitted_page.records().len(), 1);
            assert_eq!(
                result_waiter.wait(),
                beryl_app::cas_projection::ContextCompactionOutcome::Failed
            );
            return;
        }
        ready.release();
        failed.wait_until_paused();
        assert!(!harness.has_local_compaction(fixture.thread));
        assert_eq!(harness.compaction_custody_in_use(), 72);

        let cleanup_page = work_page(fixture);
        assert_eq!(cleanup_page.records().len(), 1);
        let cleanup = cleanup_page.records()[0].compaction.as_ref().unwrap();
        assert!(!cleanup.local_registered);
        assert_eq!(
            cleanup.command,
            Some(beryl_app::cas_projection::CompactionCommandWorkStage::Cleanup)
        );
        assert_eq!(
            cleanup.result,
            Some(beryl_app::cas_projection::ContextCompactionOutcome::Failed)
        );
        assert_eq!(
            fixture
                .store
                .validate_compaction_work_revision(admitted_page.revision()),
            Err(beryl_app::cas_projection::CompactionWorkError::StaleRevision)
        );

        let replacement_server = NormalTerminalServer::spawn_admission_only();
        let replacement_connector = ManagedBackendClientConnector::for_lifecycle_test(
            replacement_server.endpoint(),
            AUTHORIZATION,
        );
        let mut replacement = fixture
            .store
            .admit_lifecycle_test_candidate(
                &replacement_connector,
                execution_binding().runtime_id(),
                CasProcessGeneration::new(37_160).unwrap(),
                Path::new(EXECUTION_ROOT),
                TIMEOUT,
            )
            .unwrap();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
        replacement_server.wait_for_admission();
        let before = fixture
            .storage
            .compaction_admission_read(&*fixture.home(), fixture.thread, syndic::point_limit())
            .unwrap();
        assert!(matches!(before, CompactionAdmissionRead::Admissible(_)));
        let denied_before = fixture
            .store
            .context_compaction_diagnostics()
            .denied_admissions();
        assert!(matches!(
            fixture
                .store
                .compact_thread(ContextCompactionRequest::new(fixture.thread, TIMEOUT),),
            Err(ContextCompactionError::Unavailable)
        ));
        assert_eq!(
            fixture
                .store
                .context_compaction_diagnostics()
                .denied_admissions(),
            denied_before + 1
        );
        assert!(matches!(
            fixture
                .storage
                .compaction_admission_read(&*fixture.home(), fixture.thread, syndic::point_limit(),)
                .unwrap(),
            CompactionAdmissionRead::Admissible(_)
        ));
        replacement.invalidate_connection();
        drop(replacement);
        replacement_server.join();
        if unwind {
            failed.unwind();
            assert!(caller.join().is_err());
        } else {
            failed.release();
            assert!(matches!(
                caller.join().unwrap(),
                Err(ContextCompactionError::AuthorityMismatch)
            ));
        }
        assert_eq!(harness.compaction_custody_in_use(), 71);
        assert!(work_page(fixture).records().is_empty());
        assert_eq!(cleanup_page.records().len(), 1);
        assert_eq!(
            result_waiter.wait(),
            beryl_app::cas_projection::ContextCompactionOutcome::Failed
        );
        let replacement_slot = harness.occupy_compaction_custody(1);
        assert_eq!(harness.compaction_custody_in_use(), 72);
        drop(replacement_slot);
    });
    drop(pressure);
    assert_eq!(harness.compaction_custody_in_use(), 0);
    drop(projection);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

fn work_page(fixture: &Fixture) -> beryl_app::cas_projection::CompactionWorkPage {
    let revision = fixture.store.compaction_work_revision().unwrap();
    let page = fixture
        .store
        .compaction_work_page(
            &revision,
            None,
            beryl_app::cas_projection::CompactionWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    let control_revision = fixture.store.control_work_revision().unwrap();
    let control = fixture
        .store
        .control_work_page(
            &control_revision,
            None,
            beryl_app::cas_projection::ControlWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(control.compaction_records(), page.records());
    page
}
