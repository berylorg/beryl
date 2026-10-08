use super::recovery_support::{fail, installed};
use super::*;
use crate::app_services::recovery_graph::RecoveryServicePreparationError;

#[test]
fn recovery_service_graph_requires_retired_custody_and_preserves_private_retry() {
    for mode in [
        "cancel",
        "precancelled",
        "theme_failure",
        "cas_failure",
        "stale",
        "foreign",
        "missing",
        "unretired",
        "unsettled",
        "cancel_during_theme",
    ] {
        let (directory, mut owner, faults) = installed();
        owner
            .graph
            .as_mut()
            .unwrap()
            .handoff
            .as_mut()
            .unwrap()
            .shutdown()
            .unwrap();
        let expected = owner.graph().unwrap().home().health().generation().unwrap();
        if mode == "unsettled" {
            super::recovery_support::install_uncertain_enrollment(
                &owner,
                owner.graph().unwrap().home(),
                owner.graph().unwrap().state(),
                owner.graph().unwrap().syndic(),
                &faults,
            );
        }
        fail(&owner, &faults);
        owner.retire_failed_service_graph(expected).unwrap();
        let mut candidate = Some(owner.recover_retired_service_home(expected).unwrap());
        let generation = candidate.as_ref().unwrap().generation();
        let reference = candidate.as_ref().unwrap().service_reference();
        let cancellation = CommandCancellation::new();
        if mode == "precancelled" {
            cancellation.cancel();
        }
        if mode == "theme_failure" {
            faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
        }
        let config = configuration();
        let requirement = config.projection.turn_start_admission_requirement();
        if mode == "cas_failure" {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let missing = if mode == "missing" {
            candidate.take()
        } else {
            None
        };
        let original = if mode == "foreign" {
            candidate.take()
        } else {
            None
        };
        let (foreign_directory, foreign_candidate, _, _, foreign_faults) = fixture();
        if mode == "foreign" {
            let home = foreign_candidate.publish().unwrap();
            foreign_faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(home.home_revision().is_err());
            candidate = Some(home.recover_same_home().unwrap());
        } else {
            foreign_candidate.close().unwrap();
        }
        let saved_retirement = if mode == "unretired" {
            owner.recovery_retirement.take()
        } else {
            None
        };
        let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
        let canceller = if mode == "cancel_during_theme" {
            let block = faults.block_next(FaultPoint::BeforeThemeWatchSpawn);
            let cancellation = cancellation.clone();
            Some(std::thread::spawn(move || {
                let reached = block.wait_until_reached(Duration::from_secs(10));
                cancellation.cancel();
                block.release();
                assert!(reached);
            }))
        } else {
            None
        };
        let result = owner.prepare_recovery_service_graph(
            if mode == "stale" {
                generation
            } else {
                expected
            },
            &mut candidate,
            config,
            SyndicTimestamp::from_unix_millis(2),
            &cancellation,
        );
        if let Some(canceller) = canceller {
            canceller.join().unwrap();
        }
        if mode == "unretired" {
            owner.recovery_retirement = saved_retirement;
        }
        let mut failure = match result {
            Ok(mut prepared) => {
                assert_eq!(mode, "cancel");
                assert!(candidate.is_none());
                assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                assert!(reference.home_revision().is_err());
                assert!(owner.graph().is_none());
                assert!(owner.process.execution_permit().commit(|| ()).is_err());
                assert!(!observation.wait_until_reached(Duration::from_millis(60)));
                let home = reference.home_id();
                assert!(
                    prepared
                        .composer_recovery_adapters(home, expected, requirement)
                        .is_err()
                );
                assert!(
                    prepared
                        .composer_recovery_adapters(
                            BerylHomeId::from_bytes([99; 16]),
                            generation,
                            requirement,
                        )
                        .is_err()
                );
                let first = prepared
                    .composer_recovery_adapters(home, generation, requirement)
                    .unwrap();
                assert!(first.matches(home, generation));
                let (_, first_marker, _, _) = first.into_parts();
                let unused = prepared
                    .composer_recovery_adapters(home, generation, requirement)
                    .unwrap();
                drop(unused);
                assert!(!first_marker.test_generation_retired());
                let second = prepared
                    .composer_recovery_adapters(home, generation, requirement)
                    .unwrap();
                let failure = prepared.cancel();
                assert!(first_marker.test_generation_retired());
                let (_, second_marker, _, _) = second.into_parts();
                assert!(second_marker.test_generation_retired());
                drop((first_marker, second_marker));
                RecoveryServicePreparationError::App(failure)
            }
            Err(failure) => failure,
        };
        let returned = !matches!(failure, RecoveryServicePreparationError::Refused(_));
        if returned {
            let evidence = format!("{failure:?}");
            assert!(
                owner
                    .return_recovery_preparation_home(generation, &mut failure)
                    .is_err()
            );
            let retirement = owner.recovery_retirement.take().unwrap();
            assert!(
                owner
                    .return_recovery_preparation_home(expected, &mut failure)
                    .is_err()
            );
            owner.recovery_retirement = Some(retirement);
            owner
                .return_recovery_preparation_home(expected, &mut failure)
                .unwrap();
            assert_eq!(evidence, format!("{failure:?}"));
            assert!(
                owner
                    .return_recovery_preparation_home(expected, &mut failure)
                    .is_err()
            );
        } else {
            assert!(
                owner
                    .return_recovery_preparation_home(expected, &mut failure)
                    .is_err()
            );
        }
        let home = match failure {
            RecoveryServicePreparationError::Refused(_) => {
                assert!(matches!(
                    mode,
                    "precancelled" | "stale" | "foreign" | "missing" | "unretired" | "unsettled"
                ));
                if mode == "missing" {
                    candidate = missing;
                }
                if mode == "foreign" {
                    candidate.take().unwrap().abort().close().unwrap();
                    candidate = original;
                }
                if mode == "unsettled" {
                    assert_eq!(owner.enrollments.pending_count(), 1);
                    let retained = candidate.as_mut().unwrap();
                    let state = BerylState::reacquire_candidate(retained).unwrap();
                    let syndic = SyndicStorage::reacquire_candidate(retained).unwrap();
                    owner
                        .settle_retired_process_work(
                            &retained.recovery_access().unwrap(),
                            &state,
                            &syndic,
                            &CommandCancellation::new(),
                        )
                        .unwrap();
                }
                assert_eq!(candidate.as_ref().unwrap().generation(), generation);
                candidate.take().unwrap().abort()
            }
            RecoveryServicePreparationError::Cas(failure) => {
                assert_eq!(mode, "cas_failure");
                assert!(candidate.is_none());
                assert!(failure.into_retry_parts().is_err());
                owner.take_retired_service_home(expected).unwrap()
            }
            RecoveryServicePreparationError::App(failure) => {
                assert!(candidate.is_none());
                match mode {
                    "cancel" | "cancel_during_theme" => {
                        assert!(matches!(failure.error(), AppServiceOpenError::Cancelled))
                    }
                    "theme_failure" => {
                        assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)))
                    }
                    _ => panic!("unexpected ancillary failure"),
                }
                assert!(failure.into_retry_parts().is_err());
                owner.take_retired_service_home(expected).unwrap()
            }
        };
        assert!(!observation.wait_until_reached(Duration::from_millis(60)));
        observation.release();
        assert_reopens(&foreign_directory);
        assert_eq!(home.health().state(), HomeHealthState::Failed);
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT
            ))
            .is_err()
        );
        let mut home = Some(home);
        owner
            .return_retired_service_home(expected, &mut home)
            .unwrap();
        let candidate = owner.recover_retired_service_home(expected).unwrap();
        assert_ne!(candidate.generation(), generation);
        candidate.abort().close().unwrap();
        assert_reopens(&directory);
    }
}
