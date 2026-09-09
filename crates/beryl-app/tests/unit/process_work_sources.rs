use super::*;
use std::sync::Arc;

#[test]
fn source_handles_do_not_retain_home_or_control_owners_after_disposal() {
    let (_directory, service, sessions) = fixture();
    let home = Arc::downgrade(service.home.as_ref().unwrap());
    let connections = Arc::downgrade(&service.connections);
    let stop = Arc::downgrade(&service.stop_coordinator);
    let compaction = Arc::downgrade(service.context_compaction.as_ref().unwrap());
    let counts = (
        home.strong_count(),
        connections.strong_count(),
        stop.strong_count(),
        compaction.strong_count(),
    );
    let sources = service.work_sources();
    let retained = sources.clone();
    assert_eq!(
        counts,
        (
            home.strong_count(),
            connections.strong_count(),
            stop.strong_count(),
            compaction.strong_count()
        )
    );
    assert!(
        sources
            .required_session_work(&sessions, &ProjectionCancellationToken::new())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        counts,
        (
            home.strong_count(),
            connections.strong_count(),
            stop.strong_count(),
            compaction.strong_count()
        )
    );
    let _ = service.close().unwrap();
    assert_eq!(
        (
            home.strong_count(),
            connections.strong_count(),
            stop.strong_count(),
            compaction.strong_count()
        ),
        (0, 0, 0, 0)
    );
    for source in [sources, retained] {
        assert!(matches!(
            source.required_session_work(&sessions, &ProjectionCancellationToken::new()),
            Err(ProcessWorkError::Closed)
        ));
    }
}

#[test]
fn targeted_observations_reject_foreign_sources_cancellation_and_durable_or_control_drift() {
    let (_directory, service, sessions) = fixture();
    let (_other_directory, other_service, other_sessions) = fixture();
    let sources = service.work_sources();
    let cancellation = ProjectionCancellationToken::new();
    assert!(matches!(
        sources.required_session_work(&other_sessions, &cancellation),
        Err(ProcessWorkError::ForeignSources)
    ));
    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        sources.required_session_work(&sessions, &cancelled),
        Err(ProcessWorkError::Cancelled)
    ));
    let read = sources.read().unwrap();
    let mut held = None;
    assert!(
        read.collect_required_session_work(&sessions, &cancellation, || {
            held = Some(
                service
                    .stop_coordinator
                    .compaction_custody
                    .reserve_continuation(thread(1), turn(1))
                    .unwrap(),
            );
        })
        .is_err()
    );
    drop(held);
    assert!(matches!(
        read.collect_required_session_work(&sessions, &cancellation, || {
            let home = service.home.as_ref().unwrap();
            let mut command = HomeCommand::new(home.home_revision().unwrap());
            command
                .add(service.storage.create_thread(
                    service.storage.revision(home).unwrap(),
                    CreateThread::ordinary(
                        thread(5),
                        SyndicDraftId::from_bytes([5; 16]),
                        binding(),
                        SyndicTimestamp::from_unix_millis(40),
                        DraftEditHistoryPolicyV1::new(64 * 1024 * 1024, 1).unwrap(),
                    ),
                ))
                .unwrap();
            assert!(matches!(
                home.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }),
        Err(ProcessWorkError::StaleRevision)
    ));
    drop(read);
    assert!(
        sources
            .required_session_work(&sessions, &cancellation)
            .unwrap()
            .is_empty()
    );
    let _ = service.command_gate.close_for_shutdown();
    assert!(matches!(
        sources.required_session_work(&sessions, &cancellation),
        Err(ProcessWorkError::Closed)
    ));
    drop(other_service);
}
