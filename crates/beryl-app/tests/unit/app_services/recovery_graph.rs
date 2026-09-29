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
        let failure = match result {
            Ok(prepared) => {
                assert_eq!(mode, "cancel");
                assert!(candidate.is_none());
                assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                assert!(reference.home_revision().is_err());
                assert!(owner.graph().is_none());
                assert!(owner.process.execution_permit().commit(|| ()).is_err());
                assert!(!observation.wait_until_reached(Duration::from_millis(60)));
                RecoveryServicePreparationError::App(prepared.cancel())
            }
            Err(failure) => failure,
        };
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
                failure
                    .into_retry_parts()
                    .unwrap_or_else(|_| panic!("CAS retirement unconfirmed"))
                    .0
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
                failure.into_retry_parts().unwrap().0
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
