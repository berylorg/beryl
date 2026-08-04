use crate::cas_projection::{
    RecoveredServicePublicationReason, connection::ConnectionRetirementOutcome,
    service_supervisor::RunningServiceSlot,
};

struct FinalPublicationFixture {
    directory: Option<tempfile::TempDir>,
    slot: Arc<RunningServiceSlot>,
    converged: Option<CandidateSetConvergedAdoptedProjectionConnectionService>,
    connection: Option<Arc<ProjectionConnection>>,
    replacement_shutdowns: Arc<AtomicUsize>,
    server: Option<admission_server::NormalTerminalServer>,
}

impl FinalPublicationFixture {
    fn new(seed: u8) -> Self {
        let (directory, faults, state, _old_shutdowns, service) = service_with_worker_capacity(8);
        let server = admission_server::NormalTerminalServer::spawn_admission_only();
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            server.endpoint(),
            admission_server::AUTHORIZATION,
        );
        let session = service
            .admit_lifecycle_test_candidate(
                &connector,
                RuntimeId::from_bytes([seed; 16]),
                CasProcessGeneration::new(84_000 + u64::from(seed)).unwrap(),
                Path::new(r"C:\work\beryl"),
                Duration::from_secs(10),
            )
            .unwrap();
        server.wait_for_admission();
        let connection = Arc::clone(session.connection());

        fail_home_through_live_command(&service, state, &faults);
        assert_eq!(
            service.persistent_failure_notification().notify(),
            PersistentFailureNotificationStatus::Joined
        );
        drop(session);
        wait_until("the Phase 86 publication cut to finish", || {
            service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
        });

        let failed_generation = service.service_generation();
        let slot = RunningServiceSlot::new_for_recovery_publication_test(service, state);
        let failed_epoch = slot
            .withdraw_for_recovery_publication_test(failed_generation)
            .unwrap();
        let (failed_service, failed_state) = match failed_epoch.into_parts() {
            Ok(parts) => parts,
            Err(_) => panic!("the withdrawn failed epoch must be exclusively slot-owned"),
        };
        let _ = failed_state;
        let handoff = match failed_service.close().unwrap() {
            ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
            ProjectionConnectionServiceCloseOutcome::Closed => {
                panic!("the failed service must retain its recovery authority")
            }
        };
        let inventory = handoff.into_recovery_inventory().unwrap();
        let home = Arc::clone(inventory.retained_home());
        let config = inventory.retained_service_config();
        let quarantine = inventory.into_pending_projection_quarantine().unwrap();
        home.recover_same_home().unwrap();
        let replacement_shutdowns = Arc::new(AtomicUsize::new(0));
        let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
            Arc::clone(&home),
            config,
            Box::new(ShutdownProbe {
                count: Arc::clone(&replacement_shutdowns),
            }),
        )
        .unwrap();
        let mut adopted = quarantine.adopt_unpublished_service(replacement).unwrap();
        assert!(adopted.startup_fence_is_closed_for_test());
        adopted.converge_recovered_startup().unwrap();
        let converged = adopted.begin_candidate_reauthentication().seal().unwrap();
        assert_eq!(converged.accepted_candidate_count(), 0);
        assert_eq!(converged.retained_connection_owner_count_for_test(), 1);
        drop(home);

        Self {
            directory: Some(directory),
            slot,
            converged: Some(converged),
            connection: Some(connection),
            replacement_shutdowns,
            server: Some(server),
        }
    }

    fn take_converged(&mut self) -> CandidateSetConvergedAdoptedProjectionConnectionService {
        self.converged.take().unwrap()
    }

    fn converged_mut(&mut self) -> &mut CandidateSetConvergedAdoptedProjectionConnectionService {
        self.converged.as_mut().unwrap()
    }

    fn connection(&self) -> &Arc<ProjectionConnection> {
        self.connection.as_ref().unwrap()
    }

    fn close_published(mut self) {
        drop(self.converged.take());
        drop(self.connection.take());
        let epoch = self
            .slot
            .take_for_recovery_publication_test()
            .expect("the successful publication remains installed in the process slot");
        let (service, state) = match epoch.into_parts() {
            Ok(parts) => parts,
            Err(_) => panic!("the published epoch must be exclusively slot-owned at shutdown"),
        };
        let _ = state;
        assert!(matches!(
            service.close().unwrap(),
            ProjectionConnectionServiceCloseOutcome::Closed
        ));
        self.server.take().unwrap().join();
        drop(self.directory.take());
    }

    fn close_after_failed_publication(mut self) {
        drop(self.converged.take());
        drop(self.connection.take());
        self.server.take().unwrap().join();
        drop(self.directory.take());
    }
}

#[test]
fn phase86_publication_first_installs_before_stable_core_retirement() {
    let mut fixture = FinalPublicationFixture::new(221);
    let stable_identity = fixture.connection().identity_observation();
    let publication = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap();

    wait_until(
        "the published replacement scheduler to pass its startup gate",
        || {
            fixture
                .slot
                .acquire_for_recovery_publication_test()
                .is_ok_and(|service| {
                    service
                        .accepted_input_scheduler_diagnostics()
                        .recovered_pending_pass_count()
                        >= 1
                })
        },
    );
    assert_eq!(fixture.connection().identity_observation(), stable_identity);
    assert_eq!(publication.accepted_candidate_count(), 0);
    assert_eq!(
        fixture
            .slot
            .diagnostics_for_recovery_publication_test()
            .current_service_generation(),
        Some(publication.adoption().new_service_generation())
    );

    let retirement = fixture
        .connection()
        .retire_authority_for_recovery_test()
        .unwrap();
    assert!(matches!(
        retirement,
        ConnectionRetirementOutcome::Complete | ConnectionRetirementOutcome::FailureRetained(_)
    ));
    assert_eq!(
        fixture
            .slot
            .diagnostics_for_recovery_publication_test()
            .current_service_generation(),
        Some(publication.adoption().new_service_generation())
    );
    fixture.close_published();
}

#[test]
fn phase86_stable_core_retirement_first_returns_terminal_owning_error() {
    let mut fixture = FinalPublicationFixture::new(222);
    assert!(matches!(
        fixture
            .connection()
            .retire_authority_for_recovery_test()
            .unwrap(),
        ConnectionRetirementOutcome::FailureRetained(_)
    ));

    let error = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        RecoveredServicePublicationReason::StableConnectionRetired
    );
    error.dispose().unwrap();
    wait_until("the retirement-first replacement service to stop", || {
        fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1
    });
    assert_eq!(
        fixture
            .slot
            .diagnostics_for_recovery_publication_test()
            .current_service_generation(),
        None
    );
    fixture.close_after_failed_publication();
}

#[test]
fn phase86_slot_failure_keeps_armed_workers_blocked_and_returns_owning_disposition() {
    let mut fixture = FinalPublicationFixture::new(223);
    fixture.slot.make_recovered_install_unavailable_for_test();

    let error = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        RecoveredServicePublicationReason::ProcessServiceSlotUnavailable
    );
    assert!(error.startup_fence_is_closed_for_test());
    assert_eq!(error.replacement_scheduler_pass_count_for_test(), Some(0));
    assert_eq!(fixture.replacement_shutdowns.load(Ordering::SeqCst), 0);
    let diagnostics = fixture.slot.diagnostics_for_recovery_publication_test();
    assert_eq!(diagnostics.current_service_generation(), None);
    assert!(!diagnostics.recovering());
    assert_eq!(diagnostics.recovery_cycles(), 0);

    error.dispose().unwrap();
    wait_until("the slot-rejected replacement service to stop", || {
        fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1
    });
    fixture.close_after_failed_publication();
}

#[test]
fn phase86_old_adoption_publisher_first_blocks_retirement_witness() {
    let mut fixture = FinalPublicationFixture::new(224);
    fixture
        .converged_mut()
        .retain_late_adoption_authority_for_test();

    let error = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        RecoveredServicePublicationReason::AdoptionPublisherWon
    );
    assert!(error.startup_fence_is_closed_for_test());
    error.dispose().unwrap();
    wait_until("the late-publisher replacement service to stop", || {
        fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1
    });
    fixture.close_after_failed_publication();
}

#[test]
fn phase86_retirement_witness_first_rejects_late_adoption_publisher() {
    let mut fixture = FinalPublicationFixture::new(225);
    fixture
        .converged_mut()
        .retain_late_after_publication_fence_for_test();

    let publication = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap();
    assert_eq!(publication.accepted_candidate_count(), 0);
    assert_eq!(
        fixture
            .slot
            .diagnostics_for_recovery_publication_test()
            .current_service_generation(),
        Some(publication.adoption().new_service_generation())
    );
    fixture.close_published();
}

#[test]
fn phase86_partial_worker_arm_failure_remains_closed_and_disposes_every_worker() {
    let mut fixture = FinalPublicationFixture::new(226);
    fixture
        .converged_mut()
        .fail_replacement_ingester_arm_for_test(0);

    let error = fixture
        .take_converged()
        .publish_recovered_service(&fixture.slot)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        RecoveredServicePublicationReason::ReplacementWorkerArm
    );
    assert!(error.startup_fence_is_closed_for_test());
    assert_eq!(error.replacement_scheduler_pass_count_for_test(), Some(0));
    assert_eq!(fixture.replacement_shutdowns.load(Ordering::SeqCst), 0);
    error.dispose().unwrap();
    wait_until("the partially armed replacement topology to stop", || {
        fixture.replacement_shutdowns.load(Ordering::SeqCst) == 1
    });
    assert_eq!(
        fixture
            .connection()
            .retire_authority_for_recovery_test()
            .unwrap(),
        ConnectionRetirementOutcome::Complete
    );
    fixture.close_after_failed_publication();
}
