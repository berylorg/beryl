struct EmptyAdoptionFixture {
    _directory: tempfile::TempDir,
    adopted: AdoptedUnpublishedProjectionConnectionService,
}

fn empty_adoption_fixture() -> EmptyAdoptionFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the publication-fence cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must yield recovery authority")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    EmptyAdoptionFixture {
        _directory: directory,
        adopted: quarantine.adopt_unpublished_service(replacement).unwrap(),
    }
}

#[test]
fn phase82_clean_adoption_fence_retires_into_one_terminal_witness() {
    let mut fixture = empty_adoption_fixture();

    assert!(fixture.adopted.retire_adoption_fence_for_test().is_ok());
    fixture
        .adopted
        .retain_late_adoption_authority_for_test();
    assert!(fixture.adopted.startup_fence_is_closed_for_test());
}

#[test]
fn phase82_late_authority_after_commit_blocks_adoption_fence_retirement() {
    let mut fixture = empty_adoption_fixture();
    fixture.adopted.retain_late_adoption_authority_for_test();

    assert_eq!(
        fixture.adopted.retire_adoption_fence_for_test(),
        Err(PersistentFailurePendingProjectionQuarantineReason::LatePublication)
    );
    assert!(fixture.adopted.startup_fence_is_closed_for_test());
}

mod final_publication {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/final_publication.rs"
    ));
}
