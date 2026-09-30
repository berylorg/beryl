use super::recovery_support::{fail, installed};
use super::*;
use crate::app_services::recovery_graph::{
    PreparedRecoveryServiceGraph, RecoveryServicePreparationError,
};
use beryl_home_store::HomeGeneration;

fn replacement(
    owner: &mut ProcessServiceOwner,
    expected: HomeGeneration,
) -> (HomeGeneration, Option<PreparedRecoveryServiceGraph>) {
    let mut candidate = Some(owner.recover_retired_service_home(expected).unwrap());
    let generation = candidate.as_ref().unwrap().generation();
    let graph = owner
        .prepare_recovery_service_graph(
            expected,
            &mut candidate,
            configuration(),
            SyndicTimestamp::from_unix_millis(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(candidate.is_none());
    (generation, Some(graph))
}

fn retired() -> (
    tempfile::TempDir,
    ProcessServiceOwner,
    FaultController,
    HomeGeneration,
) {
    let (directory, mut owner, faults) = installed();
    eprintln!(
        "recovery publication fixture: {}",
        directory.path().display()
    );
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
    fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    (directory, owner, faults, expected)
}

fn dispose(
    owner: &mut ProcessServiceOwner,
    expected: HomeGeneration,
    graph: PreparedRecoveryServiceGraph,
) {
    let mut failure = RecoveryServicePreparationError::App(graph.cancel());
    owner
        .return_recovery_preparation_home(expected, &mut failure)
        .unwrap();
}

#[test]
fn prepared_graph_session_validation_refuses_unproven_outcome_without_consuming_custody() {
    let (directory, mut owner, _faults, expected) = retired();
    let (generation, mut prepared) = replacement(&mut owner, expected);
    let graph = prepared.as_mut().unwrap();
    let appearance = graph.appearance();
    let original = crate::running_owner::RunningShutdownSession::Unwound;
    for (home, generation, reason) in [
        (owner.home_id, expected, "another recovery candidate"),
        (
            BerylHomeId::from_bytes([99; 16]),
            generation,
            "another recovery candidate",
        ),
        (owner.home_id, generation, "unproven"),
    ] {
        assert!(
            graph
                .revalidate_interrupted_exit_session(home, generation, &original)
                .unwrap_err()
                .contains(reason)
        );
        assert!(Arc::ptr_eq(&appearance, &graph.appearance()));
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
    }
    assert!(graph.matches_candidate(owner.home_id, generation));
    dispose(&mut owner, expected, prepared.take().unwrap());
    owner
        .take_retired_service_home(expected)
        .unwrap()
        .close()
        .unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}

#[test]
fn recovery_publication_installs_complete_graph_without_releasing_interaction() {
    for release in [false, true] {
        let (directory, mut owner, faults, expected) = retired();
        let (generation, mut prepared) = replacement(&mut owner, expected);
        let appearance = prepared.as_ref().unwrap().appearance();
        let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
        let start = owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new(),
            )
            .unwrap();
        assert!(prepared.is_none());
        assert!(owner.recovery_retirement.is_none());
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
        let graph = owner.graph_mut().unwrap();
        assert_eq!(graph.home().health().state(), HomeHealthState::Healthy);
        assert_eq!(graph.home().health().generation(), Some(generation));
        assert!(graph.home().home_revision().is_ok());
        assert!(graph.activity.is_some() && graph.marker.is_some() && graph.cas.is_some());
        assert!(Arc::ptr_eq(
            &appearance,
            &graph.current_appearance().unwrap()
        ));
        assert_eq!(graph.handoff.as_ref().unwrap().test_completed_passes(), 0);
        assert!(!observation.wait_until_reached(Duration::from_millis(60)));
        let attention = Arc::clone(graph.attention());
        let tracked = attention.track_accepted_yield(
            graph.home().home_id(),
            beryl_model::SyndicThreadId::from_bytes([7; 16]),
            beryl_model::SyndicTurnId::from_bytes([8; 16]),
            crate::LifecycleYieldOutcome::PlanComplete,
        );
        assert!(
            tracked.is_some(),
            "publication preserves the live attention owner"
        );
        if release {
            assert!(start.release());
            let deadline = Instant::now() + Duration::from_secs(5);
            while graph.handoff.as_ref().unwrap().test_completed_passes() == 0 {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            graph.release_theme().unwrap();
            assert!(graph.theme.is_none());
            assert!(Arc::ptr_eq(
                &appearance,
                &graph.current_appearance().unwrap()
            ));
            assert!(graph.release_theme().is_err());
            assert!(Arc::ptr_eq(
                &appearance,
                &graph.current_appearance().unwrap()
            ));
            assert!(observation.wait_until_reached(Duration::from_secs(3)));
        } else {
            drop(start);
        }
        observation.release();
        if release {
            graph.theme().unwrap().retire();
            assert!(graph.current_appearance().is_none());
        }
        assert!(
            owner
                .publish_recovery_service_graph(
                    expected,
                    generation,
                    &mut prepared,
                    &CommandCancellation::new(),
                )
                .is_err()
        );
        assert!(owner.process.execution_permit().commit(|| ()).is_err());
        if release {
            owner
                .graph
                .as_mut()
                .unwrap()
                .handoff
                .as_mut()
                .unwrap()
                .shutdown()
                .unwrap();
            fail(&owner, &faults);
            owner.retire_failed_service_graph(generation).unwrap();
            owner
                .take_retired_service_home(generation)
                .unwrap()
                .close()
                .unwrap();
        } else {
            owner.graph.take().unwrap().dispose_unstarted().unwrap();
        }
        assert!(attention.snapshot().is_empty());
        assert_reopens(&directory);
        directory.close().unwrap();
    }
}

#[test]
fn recovery_publication_refuses_stale_or_cancelled_attachment_without_consuming_graph() {
    let (directory, mut owner, _, expected) = retired();
    let (generation, mut prepared) = replacement(&mut owner, expected);
    let cancel = CommandCancellation::new();
    cancel.cancel();
    assert!(
        owner
            .publish_recovery_service_graph(expected, generation, &mut prepared, &cancel)
            .is_err()
    );
    for (old, fresh) in [(generation, generation), (expected, expected)] {
        assert!(
            owner
                .publish_recovery_service_graph(
                    old,
                    fresh,
                    &mut prepared,
                    &CommandCancellation::new()
                )
                .is_err()
        );
    }
    let process = std::mem::replace(&mut owner.process, ProcessAdmissionGate::new());
    assert!(
        owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new()
            )
            .is_err()
    );
    owner.process = process;
    let retirement = owner.recovery_retirement.take();
    assert!(
        owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new()
            )
            .is_err()
    );
    owner.recovery_retirement = retirement;
    assert!(
        prepared
            .as_mut()
            .unwrap()
            .matches_candidate(owner.home_id, generation)
    );
    assert!(owner.graph().is_none());
    assert!(owner.process.execution_permit().commit(|| ()).is_err());
    dispose(&mut owner, expected, prepared.take().unwrap());
    owner
        .take_retired_service_home(expected)
        .unwrap()
        .close()
        .unwrap();
    assert_reopens(&directory);
}

#[test]
fn recovery_publication_failure_retains_disposal_and_fresh_retry_custody() {
    let (directory, mut owner, faults, expected) = retired();
    let (generation, mut prepared) = replacement(&mut owner, expected);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        prepared
            .as_mut()
            .unwrap()
            .threadless_recovery_window(
                owner.home_id,
                expected,
                beryl_model::WindowId::from_bytes([9; 16]),
            )
            .is_err()
    );
    assert!(
        owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new()
            )
            .is_err()
    );
    assert!(prepared.is_some() && owner.graph().is_none());
    dispose(&mut owner, expected, prepared.take().unwrap());
    let (fresh, mut prepared) = replacement(&mut owner, expected);
    assert_ne!(generation, fresh);
    assert!(
        owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new()
            )
            .is_err()
    );
    let start = owner
        .publish_recovery_service_graph(expected, fresh, &mut prepared, &CommandCancellation::new())
        .unwrap();
    drop(start);
    owner.graph.take().unwrap().dispose_unstarted().unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}

#[test]
fn recovery_publication_refuses_foreign_prepared_graph() {
    let (directory, mut owner, _, expected) = retired();
    let (foreign_directory, mut foreign, _, foreign_expected) = retired();
    let (generation, mut prepared) = replacement(&mut foreign, foreign_expected);
    let home = owner.take_retired_service_home(expected).unwrap();
    assert!(
        owner
            .publish_recovery_service_graph(
                expected,
                generation,
                &mut prepared,
                &CommandCancellation::new()
            )
            .is_err()
    );
    assert!(owner.graph().is_none());
    assert!(
        prepared
            .as_mut()
            .unwrap()
            .matches_candidate(foreign.home_id, generation)
    );
    dispose(&mut foreign, foreign_expected, prepared.take().unwrap());
    foreign
        .take_retired_service_home(foreign_expected)
        .unwrap()
        .close()
        .unwrap();
    home.close().unwrap();
    assert_reopens(&directory);
    assert_reopens(&foreign_directory);
}

#[test]
fn recovery_admission_retains_exact_fence_until_successful_settlement() {
    let (directory, mut owner, faults) = installed();
    let expected = owner.graph().unwrap().home().health().generation().unwrap();
    assert!(owner.reopen_recovery_admission(expected).is_err());
    let predecessor = owner.process.execution_permit();
    let reservation = predecessor.reserve().unwrap();
    owner
        .graph
        .as_mut()
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .shutdown()
        .unwrap();
    fail(&owner, &faults);
    owner.retire_failed_service_graph(expected).unwrap();
    assert!(owner.reopen_recovery_admission(expected).is_err());
    let (generation, mut prepared) = replacement(&mut owner, expected);
    let start = owner
        .publish_recovery_service_graph(
            expected,
            generation,
            &mut prepared,
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(owner.reopen_recovery_admission(expected).is_err());
    assert!(
        owner
            .reopen_recovery_admission(generation)
            .unwrap_err()
            .contains("not settled")
    );
    assert!(matches!(
        owner.attempt,
        InitialServiceAttemptState::Published(Some(_))
    ));
    assert!(owner.process.execution_permit().reserve().is_err());
    assert_eq!(
        owner
            .graph()
            .unwrap()
            .handoff
            .as_ref()
            .unwrap()
            .test_completed_passes(),
        0
    );
    drop(reservation);
    owner.reopen_recovery_admission(generation).unwrap();
    assert!(matches!(
        owner.attempt,
        InitialServiceAttemptState::Published(None)
    ));
    assert!(predecessor.reserve().is_err());
    drop(owner.process.execution_permit().reserve().unwrap());
    assert!(owner.reopen_recovery_admission(generation).is_err());
    let next = owner.process.fence().unwrap();
    assert!(owner.reopen_recovery_admission(generation).is_err());
    assert!(owner.process.execution_permit().reserve().is_err());
    drop(next);
    drop(start);
    owner.graph.take().unwrap().dispose_unstarted().unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}

#[test]
fn recovery_admission_preserves_fence_when_published_home_fails() {
    let (directory, mut owner, faults, expected) = retired();
    let (generation, mut prepared) = replacement(&mut owner, expected);
    let start = owner
        .publish_recovery_service_graph(
            expected,
            generation,
            &mut prepared,
            &CommandCancellation::new(),
        )
        .unwrap();
    fail(&owner, &faults);
    for _ in 0..2 {
        assert!(owner.reopen_recovery_admission(generation).is_err());
        assert!(matches!(
            owner.attempt,
            InitialServiceAttemptState::Published(Some(_))
        ));
        assert!(owner.process.execution_permit().reserve().is_err());
    }
    drop(start);
    owner.graph.take().unwrap().dispose_unstarted().unwrap();
    assert_reopens(&directory);
    directory.close().unwrap();
}
