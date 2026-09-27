use super::*;
use crate::{
    cas_projection::{ProjectionCancellationToken, ShutdownFailure},
    support,
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use syndic_storage::SyndicPointReadLimit;

use super::recovery_support::install_uncertain_enrollment;

#[test]
fn shutdown_refuses_uncertain_enrollment_and_keeps_the_original_process_custody() {
    let (directory, candidate, state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            support::timestamp(1),
            CommandCancellation::new(),
        )
        .unwrap();
    let _fence = owner.process.fence().unwrap();
    let graph = owner.graph().unwrap();
    let runtime =
        install_uncertain_enrollment(&owner, graph.home.as_ref().unwrap(), &graph.syndic, &faults);
    let original = owner
        .graph()
        .unwrap()
        .home
        .as_ref()
        .unwrap()
        .pending_reconciliations()
        .pop()
        .unwrap();
    owner.begin_shutdown().unwrap();
    let cancellation = ProjectionCancellationToken::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let result = loop {
        let result = owner.poll_shutdown(&cancellation);
        if matches!(result, Ok(AppServiceShutdownProgress::Waiting))
            && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
            continue;
        }
        break result;
    };
    let retained = owner.enrollments.pending_count();
    let graph = owner.graph().expect("failed shutdown retains the graph");
    let home = graph.home.as_ref().unwrap();
    let still_pending = home.pending_reconciliations();
    assert_eq!(still_pending.len(), 1);
    let original_result = home.retry_reconciliation(&original).unwrap();
    assert!(matches!(
        original_result,
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    owner
        .enrollments
        .settle_retired_runtime(home, &graph.syndic, runtime)
        .unwrap();
    assert!(
        matches!(
            result,
            Err(AppServiceCloseError::PendingCustody {
                enrollments: 1,
                nondispatch: 0
            })
        ),
        "shutdown must reject retained custody explicitly: {result:?}; retained enrollments: {retained}; pending registry scopes before cleanup: {}",
        still_pending.len(),
    );
    assert_eq!(retained, 1);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert!(home.pending_reconciliations().is_empty());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let after_settlement = loop {
        let progress = owner.poll_shutdown(&cancellation).unwrap();
        if matches!(
            progress,
            AppServiceShutdownProgress::Waiting
                | AppServiceShutdownProgress::Failed {
                    reopened: false,
                    ..
                }
        ) && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
            continue;
        }
        break progress;
    };
    assert!(
        matches!(
            after_settlement,
            AppServiceShutdownProgress::Failed {
                reason: ShutdownFailure::UnprovenExecution { thread, turn },
                reopened: true,
            } if thread == support::id(40) && turn == support::populated::active_turn()
        ),
        "the unrelated fixture active turn still has no execution custody: {after_settlement:?}",
    );
    let graph = owner.graph().unwrap();
    assert!(
        graph
            .syndic
            .pending_dispatch_evidence(
                graph.home.as_ref().unwrap(),
                support::id(30),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .is_some()
    );
    drop(owner.graph.take());
    assert!(owner.graph().is_none());
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert_reopens(&directory);
}

#[test]
fn rejected_foreign_initial_candidate_does_not_discard_retained_enrollment() {
    let (_directory, candidate, _state, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    let home = candidate.publish().unwrap();
    let runtime = install_uncertain_enrollment(&owner, &home, &syndic, &faults);
    let original = home.pending_reconciliations().pop().unwrap();
    let (_foreign_directory, foreign, foreign_state, foreign_syndic, _) = fixture();
    let result = owner.open_initial(
        foreign,
        foreign_state,
        foreign_syndic,
        configuration(),
        support::timestamp(101),
        CommandCancellation::new(),
    );
    let retained = owner.enrollments.pending_count();
    let pending = home.pending_reconciliations().len();
    assert!(matches!(
        home.retry_reconciliation(&original).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    owner
        .enrollments
        .settle_retired_runtime(&home, &syndic, runtime)
        .unwrap();
    let failure = result.unwrap_err();
    assert!(matches!(failure.error, AppServiceOpenError::ForeignHome));
    failure.rejected_candidate.unwrap().close().unwrap();
    assert!(owner.graph().is_none());
    assert_eq!(retained, 1);
    assert_eq!(pending, 1);
    assert_eq!(owner.enrollments.pending_count(), 0);
    assert!(home.pending_reconciliations().is_empty());
    home.close().unwrap();
}

#[test]
fn same_home_candidate_cannot_replace_pending_process_enrollment_custody() {
    let (directory, candidate, _, syndic, faults) = fixture();
    let mut owner = owner(&candidate);
    let home = candidate.publish().unwrap();
    install_uncertain_enrollment(&owner, &home, &syndic, &faults);
    let original = home.pending_reconciliations().pop().unwrap();
    assert!(matches!(
        home.retry_reconciliation(&original).unwrap(),
        beryl_home_store::ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(owner.enrollments.pending_count(), 1);
    home.close().unwrap();
    let (candidate, state, syndic) = super::reopening::candidate_at(&directory);
    let reference = candidate.service_reference();
    let failure = owner
        .open_initial(
            candidate,
            state,
            syndic,
            configuration(),
            support::timestamp(101),
            CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        failure.error,
        AppServiceOpenError::UnsettledCustody(AppServiceCloseError::PendingCustody {
            enrollments: 1,
            nondispatch: 0
        })
    ));
    assert_eq!(owner.enrollments.pending_count(), 1);
    assert!(owner.graph().is_none());
    let candidate = failure.rejected_candidate.unwrap();
    assert_eq!(candidate.home_id(), owner.home_id);
    let home = candidate.publish().unwrap();
    assert!(reference.home_revision().is_ok());
    assert_eq!(owner.enrollments.pending_count(), 1);
    drop(owner);
    home.close().unwrap();
}
