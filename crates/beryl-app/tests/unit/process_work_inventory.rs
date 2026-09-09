use super::*;
use crate::{
    LifecycleYieldOutcome,
    cas_projection::{
        MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider, ProjectionServiceConfig,
    },
    lifecycle_attention::{LifecycleAttentionAdmission, LifecycleAttentionWorkError},
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicTurnId,
};
use beryl_state::BerylState;
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
fn turn(seed: u8) -> SyndicTurnId {
    SyndicTurnId::from_bytes([seed; 16])
}

fn binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([1; 16]),
        RootId::from_bytes([2; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    )
}

fn fixture() -> (
    tempfile::TempDir,
    ProjectionConnectionService,
    ScheduledExecutionSessions,
) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    BerylState::register(&mut home).unwrap();
    for (seed, activity) in [(1, 20), (2, 30), (3, 20), (4, 10)] {
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread(seed),
                    SyndicDraftId::from_bytes([seed; 16]),
                    binding(),
                    SyndicTimestamp::from_unix_millis(activity),
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
    }
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(provider),
    )
    .unwrap();
    (directory, service, sessions)
}

fn report(pool: &ProcessLifecycleAttentionPool, service: &ProjectionConnectionService, seed: u8) {
    let attempt = pool
        .track_accepted_yield(
            service.home_id,
            thread(seed),
            turn(seed),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    assert!(matches!(
        pool.report_terminal(&attempt),
        LifecycleAttentionAdmission::Admitted(_)
    ));
}

fn limits(records: usize, bytes: usize) -> ProcessWorkPageLimits {
    ProcessWorkPageLimits::new(records, bytes).unwrap()
}

#[test]
fn inventory_deduplicates_attention_and_custody_with_stable_counts_and_no_side_effects() {
    let (_directory, service, sessions) = fixture();
    let attention = ProcessLifecycleAttentionPool::new();
    for seed in [1, 2, 3] {
        report(&attention, &service, seed);
    }
    let held = service
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(thread(1), turn(9))
        .unwrap();
    let inventory = service.process_work_inventory(&sessions, &attention);
    let cancellation = ProjectionCancellationToken::new();
    let revision = inventory.revision().unwrap();
    let home_revision = inventory.home().unwrap().home_revision().unwrap();
    let all = inventory
        .page(&revision, None, limits(256, 65_536), &cancellation)
        .unwrap();
    assert_eq!(all.total_threads(), 3);
    assert_eq!(
        all.records()
            .iter()
            .map(|row| row.thread_id)
            .collect::<Vec<_>>(),
        [thread(2), thread(1), thread(3)]
    );
    assert!(all.records()[1].facts.continuation);
    assert!(
        all.records()
            .iter()
            .all(|row| row.attention.len() == 1 && row.execution == binding())
    );
    assert!(all.next_cursor().is_none());
    let mut cursor = None;
    let mut paged = Vec::new();
    for index in 0..3 {
        let page = inventory
            .page(&revision, cursor.as_ref(), limits(1, 65_536), &cancellation)
            .unwrap();
        assert_eq!(page.total_threads(), 3);
        assert_eq!(page.next_cursor().is_some(), index < 2);
        paged.extend_from_slice(page.records());
        cursor = page.next_cursor().cloned();
    }
    assert_eq!(paged, all.records());
    let first_bytes = all.records()[0].bytes();
    let first = inventory
        .page(&revision, None, limits(256, first_bytes), &cancellation)
        .unwrap();
    assert_eq!(first.records().len(), 1);
    assert_eq!(first.bytes(), first_bytes);
    assert!(matches!(
        inventory.page(&revision, None, limits(256, first_bytes - 1), &cancellation),
        Err(ProcessWorkError::ByteLimit)
    ));
    assert_eq!(inventory.revision().unwrap(), revision);
    assert_eq!(
        inventory.home().unwrap().home_revision().unwrap(),
        home_revision
    );
    assert_eq!(attention.snapshot().len(), 3);
    drop(held);
    assert!(inventory.validate_revision(&revision).is_err());
    let current = inventory.revision().unwrap();
    let after = inventory
        .page(&current, None, limits(256, 65_536), &cancellation)
        .unwrap();
    assert_eq!(after.total_threads(), 3);
    assert!(after.records().iter().all(|row| !row.facts.continuation));
    for row in all.records() {
        assert!(attention.acknowledge(row.attention[0].token()));
    }
    let empty = inventory.revision().unwrap();
    assert_eq!(
        inventory
            .page(&empty, None, limits(1, 1), &cancellation)
            .unwrap()
            .total_threads(),
        0
    );
}

#[test]
fn inventory_rejects_foreign_sources_cursors_cancellation_and_drift_at_return() {
    let (_directory, service, sessions) = fixture();
    let (_other_directory, other_service, other_sessions) = fixture();
    let attention = ProcessLifecycleAttentionPool::new();
    report(&attention, &service, 1);
    report(&attention, &service, 2);
    let inventory = service.process_work_inventory(&sessions, &attention);
    assert!(matches!(
        service
            .process_work_inventory(&other_sessions, &attention)
            .revision(),
        Err(ProcessWorkError::ForeignSources)
    ));
    let cancellation = ProjectionCancellationToken::new();
    let revision = inventory.revision().unwrap();
    let first = inventory
        .page(&revision, None, limits(1, 65_536), &cancellation)
        .unwrap();
    assert!(
        other_service
            .process_work_inventory(&other_sessions, &attention)
            .validate_revision(&revision)
            .is_err()
    );
    let other_attention = ProcessLifecycleAttentionPool::new();
    assert!(matches!(
        service
            .process_work_inventory(&sessions, &other_attention)
            .validate_revision(&revision),
        Err(ProcessWorkError::Attention(
            LifecycleAttentionWorkError::ForeignRevision
        ))
    ));
    let token = first.records()[0].attention[0].token();
    let result =
        inventory.collect_page(&revision, None, limits(256, 65_536), &cancellation, || {
            assert!(attention.acknowledge(token));
        });
    assert!(matches!(
        result,
        Err(ProcessWorkError::Attention(
            LifecycleAttentionWorkError::StaleRevision
        ))
    ));
    let current = inventory.revision().unwrap();
    assert!(matches!(
        inventory.page(
            &current,
            first.next_cursor(),
            limits(256, 65_536),
            &cancellation
        ),
        Err(ProcessWorkError::ForeignCursor)
    ));
    assert!(matches!(
        inventory.collect_page(&current, None, limits(256, 65_536), &cancellation, || {
            cancellation.cancel()
        }),
        Err(ProcessWorkError::Cancelled)
    ));
    assert!(matches!(
        inventory.page(&current, None, limits(256, 65_536), &cancellation),
        Err(ProcessWorkError::Cancelled)
    ));
    assert_eq!(attention.snapshot().len(), 1);
    assert_eq!(inventory.revision().unwrap(), current);
    attention.close();
    assert!(matches!(
        inventory.revision(),
        Err(ProcessWorkError::Attention(
            LifecycleAttentionWorkError::Closed
        ))
    ));
}

#[test]
fn inventory_rejects_missing_canonical_metadata_and_control_changes_during_scan() {
    let (_directory, service, sessions) = fixture();
    let attention = ProcessLifecycleAttentionPool::new();
    let inventory = service.process_work_inventory(&sessions, &attention);
    let cancellation = ProjectionCancellationToken::new();
    let revision = inventory.revision().unwrap();
    let mut held = None;
    assert!(
        inventory
            .collect_page(&revision, None, limits(256, 65_536), &cancellation, || {
                held = service
                    .stop_coordinator
                    .compaction_custody
                    .reserve_continuation(thread(1), turn(1));
            })
            .is_err()
    );
    drop(held);
    let revision = inventory.revision().unwrap();
    assert!(matches!(
        inventory.collect_page(&revision, None, limits(256, 65_536), &cancellation, || {
            let home = inventory.home().unwrap();
            let mut command = HomeCommand::new(home.home_revision().unwrap());
            command
                .add(service.storage.create_thread(
                    service.storage.revision(home).unwrap(),
                    CreateThread::ordinary(
                        thread(8),
                        SyndicDraftId::from_bytes([8; 16]),
                        binding(),
                        SyndicTimestamp::from_unix_millis(50),
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
    report(&attention, &service, 9);
    let revision = inventory.revision().unwrap();
    assert!(matches!(
        inventory.page(&revision, None, limits(256, 65_536), &cancellation),
        Err(ProcessWorkError::MissingMetadata)
    ));
    assert_eq!(attention.snapshot().len(), 1);
}

#[test]
fn inventory_rejects_queries_after_service_admission_closes() {
    let (_directory, service, sessions) = fixture();
    let attention = ProcessLifecycleAttentionPool::new();
    let inventory = service.process_work_inventory(&sessions, &attention);
    let revision = inventory.revision().unwrap();
    let _ = service.command_gate.close_for_shutdown();
    assert!(matches!(
        inventory.revision(),
        Err(ProcessWorkError::Sessions(
            crate::cas_projection::ScheduledSessionWorkError::Closed
        ))
    ));
    assert!(matches!(
        inventory.page(
            &revision,
            None,
            limits(256, 65_536),
            &ProjectionCancellationToken::new()
        ),
        Err(ProcessWorkError::Sessions(
            crate::cas_projection::ScheduledSessionWorkError::Closed
        ))
    ));
}

#[test]
fn inventory_source_failure_discards_selected_rows_without_releasing_work() {
    let (_directory, service, sessions) = fixture();
    let attention = ProcessLifecycleAttentionPool::new();
    report(&attention, &service, 1);
    let pool = &service.stop_coordinator.compaction_custody;
    let held = pool.reserve_continuation(thread(1), turn(1)).unwrap();
    let inventory = service.process_work_inventory(&sessions, &attention);
    let revision = inventory.revision().unwrap();
    let result = inventory.collect_page(
        &revision,
        None,
        limits(256, 65_536),
        &ProjectionCancellationToken::new(),
        || {
            for seed in 10..91 {
                pool.source.begin(thread(seed), Some(turn(seed)));
            }
        },
    );
    assert!(matches!(
        result,
        Err(ProcessWorkError::Controls(
            crate::cas_projection::ControlWorkError::Compaction(
                crate::cas_projection::CompactionWorkError::RevisionUnavailable
            )
        ))
    ));
    assert_eq!(pool.in_use(), 1);
    assert_eq!(attention.snapshot().len(), 1);
    drop(held);
    assert_eq!(pool.in_use(), 0);
}
