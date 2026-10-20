use super::*;
use beryl_model::{DomainRevision, SyndicThreadId};
use syndic_storage::RecoveryProjectionError;

#[test]
fn only_qualified_seed_or_abandoned_target_advances_restart_pending_execution() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = crate::support::open(directory.path());
    syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(syndic_storage::SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let generation = store.health().generation().unwrap();
    let before_injection = || ProjectionExecutionError::ProjectionSourceAdvancedBeforeInjection {
        thread_id: SyndicThreadId::from_bytes([91; 16]),
        expected: DomainRevision::new(2).unwrap(),
        actual: DomainRevision::new(3).unwrap(),
    };
    for witnessed in [
        ProjectionExecutionError::NativeContextSeedSourceAdvanced {
            thread_id: SyndicThreadId::from_bytes([91; 16]),
            expected: DomainRevision::new(2).unwrap(),
            actual: DomainRevision::new(3).unwrap(),
        },
        before_injection(),
    ] {
        assert!(matches!(
            classify_projection_error(witnessed, generation),
            PendingTurnExecutionDisposition::SourceAdvanced
        ));
    }
    for error in [
        ProjectionExecutionError::RecoveryProjection(RecoveryProjectionError::ConcurrentChange),
        ProjectionExecutionError::InjectionCompletionUnknown {
            thread_id: beryl_model::CasThreadId::new("partly-written-context-target").unwrap(),
            source: Box::new(beryl_backend::ManagedBackendError::ThreadInjectionSource {
                method: "thread/inject_items".into(),
                source: beryl_backend::ThreadInjectionSourceError::RevisionDrift,
                transport_bytes_written: true,
            }),
        },
        ProjectionExecutionError::InjectionNotDispatched {
            thread_id: beryl_model::CasThreadId::new("unwitnessed-context-target").unwrap(),
            source: Box::new(beryl_backend::ManagedBackendError::ThreadInjectionSource {
                method: "thread/inject_items".into(),
                source: beryl_backend::ThreadInjectionSourceError::RevisionDrift,
                transport_bytes_written: false,
            }),
        },
        ProjectionExecutionError::AbandonmentFailed {
            primary: Box::new(ProjectionExecutionError::RecoveryProjection(
                RecoveryProjectionError::ConcurrentChange,
            )),
            release: None,
            publication: None,
        },
    ] {
        assert!(matches!(
            classify_projection_error(error, generation),
            PendingTurnExecutionDisposition::ProjectionRefused
        ));
    }
    for (release_failed, publication_failed) in [(true, false), (false, true), (true, true)] {
        let wrapped = ProjectionExecutionError::AbandonmentFailed {
            primary: Box::new(before_injection()),
            release: release_failed.then(|| {
                Box::new(LoadedProjectionReleaseError::Registry(
                    ProjectionCoordinatorError::ProjectionWorkerStopped,
                ))
            }),
            publication: publication_failed
                .then(|| Box::new(crate::cas_projection::ProjectionPublicationFailure::Collision)),
        };
        assert!(matches!(
            classify_projection_error(wrapped, generation),
            PendingTurnExecutionDisposition::ProjectionRefused
        ));
    }
    assert!(matches!(
        classify_projection_error(ProjectionExecutionError::Cancelled, generation),
        PendingTurnExecutionDisposition::ExpectedInterruption
    ));
    store.close().unwrap();
}

#[test]
fn only_bare_pre_acquisition_retained_cleanup_is_scoped_unavailable_and_home_cuts_win() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = crate::support::open(directory.path());
    syndic_storage::SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(syndic_storage::SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let generation = store.health().generation().unwrap();
    let refused_release = || {
        Box::new(LoadedProjectionReleaseError::Registry(
            ProjectionCoordinatorError::ProjectionWorkerStopped,
        ))
    };
    assert!(matches!(
        classify_projection_error(
            ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(refused_release()),
            generation
        ),
        PendingTurnExecutionDisposition::RetainedProjectionUnavailable
    ));
    assert!(matches!(
        classify_projection_error(
            ProjectionExecutionError::LeaseRelease(refused_release()),
            generation
        ),
        PendingTurnExecutionDisposition::ProjectionRefused
    ));
    assert!(matches!(
        classify_projection_error(
            ProjectionExecutionError::AbandonmentFailed {
                primary: Box::new(
                    ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(
                        refused_release()
                    )
                ),
                release: Some(refused_release()),
                publication: None,
            },
            generation
        ),
        PendingTurnExecutionDisposition::ProjectionRefused
    ));
    let cut = || {
        Box::new(LoadedProjectionReleaseError::Registry(
            ProjectionCoordinatorError::LiveCommandPersistentHomeFailure { generation },
        ))
    };
    for error in [
        ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(cut()),
        ProjectionExecutionError::AbandonmentFailed {
            primary: Box::new(
                ProjectionExecutionError::RetainedProjectionReleaseBeforeAcquisition(
                    refused_release(),
                ),
            ),
            release: Some(cut()),
            publication: None,
        },
    ] {
        assert!(matches!(
            classify_projection_error(error, generation),
            PendingTurnExecutionDisposition::PersistentHomeFailure
        ));
    }
    store.close().unwrap();
}
