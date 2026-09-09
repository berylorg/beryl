use super::*;
use crate::cas_projection::{
    MinimumTurnCaptureReserve, PermissionInterruptionWorkFact, PermissionInterruptionWorkStage,
    ProjectionServiceConfig, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionProvider,
    ScheduledOrdinaryExecutionUnavailable, StopWorkError, stop::PermissionCustodyToken,
};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

struct IdleProvider;

impl ScheduledOrdinaryExecutionProvider for IdleProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }
    fn shutdown(&mut self) {}
}

fn service() -> (tempfile::TempDir, ProjectionConnectionService) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(IdleProvider),
    )
    .unwrap();
    (directory, service)
}

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}

fn turn(seed: u8) -> SyndicTurnId {
    SyndicTurnId::from_bytes([seed; 16])
}

fn permission(service: &ProjectionConnectionService, seed: u8) -> PermissionCustodyToken {
    service
        .stop_coordinator
        .observe_permission(PermissionInterruptionWorkFact {
            serial: 0,
            operation_id: None,
            runtime_id: RuntimeId::from_bytes([1; 16]),
            connection_generation: 1,
            registration_serial: seed as u64,
            thread_id: thread(seed),
            loaded_generation: CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            cas_thread_id: CasThreadId::new("control-work-thread").unwrap(),
            cas_turn_id: CasTurnId::new("control-work-turn").unwrap(),
            cas_item_id: None,
            stage: PermissionInterruptionWorkStage::Reserved,
        })
        .unwrap()
}

fn limits(records: usize, bytes: usize) -> ControlWorkPageLimits {
    ControlWorkPageLimits::new(records, bytes).unwrap()
}

#[test]
fn composed_pages_share_budgets_cross_sources_and_retain_no_capacity() {
    let (_directory, service) = service();
    let permissions = [permission(&service, 1), permission(&service, 2)];
    let pool = &service.stop_coordinator.compaction_custody;
    let continuations = [
        pool.reserve_continuation(thread(3), turn(3)).unwrap(),
        pool.reserve_continuation(thread(4), turn(4)).unwrap(),
    ];
    let home_before = service.home.as_ref().unwrap().home_revision().unwrap();
    let revision = service.control_work_revision().unwrap();
    let all = service
        .control_work_page(&revision, None, limits(256, 65_536))
        .unwrap();
    assert_eq!(
        (all.stop_records().len(), all.compaction_records().len()),
        (2, 2)
    );
    assert!(all.next_cursor().is_none());
    assert_eq!(all.revision(), &revision);
    assert_eq!(service.control_work_revision().unwrap(), revision);
    let mut cursor = None;
    let mut stops = Vec::new();
    let mut compactions = Vec::new();
    for index in 0..4 {
        let page = service
            .control_work_page(&revision, cursor.as_ref(), limits(1, 65_536))
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page.next_cursor().is_some(), index < 3);
        stops.extend_from_slice(page.stop_records());
        compactions.extend_from_slice(page.compaction_records());
        cursor = page.next_cursor().cloned();
    }
    assert_eq!(stops, all.stop_records());
    assert_eq!(compactions, all.compaction_records());
    let stop_bytes = service
        .stop_work_page(
            &revision.stop,
            None,
            StopWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap()
        .bytes();
    let compaction_bytes = service
        .compaction_work_page(
            &revision.compaction,
            None,
            CompactionWorkPageLimits::new(1, 65_536).unwrap(),
        )
        .unwrap()
        .bytes();
    for budget in [
        limits(2, 65_536),
        limits(256, stop_bytes),
        limits(256, stop_bytes + compaction_bytes - 1),
    ] {
        let first = service.control_work_page(&revision, None, budget).unwrap();
        assert_eq!(first.stop_records(), all.stop_records());
        assert!(first.compaction_records().is_empty());
        let next = service
            .control_work_page(&revision, first.next_cursor(), limits(256, 65_536))
            .unwrap();
        assert!(next.stop_records().is_empty());
        assert_eq!(next.compaction_records(), all.compaction_records());
        assert!(next.next_cursor().is_none());
    }
    let exact = service
        .control_work_page(&revision, None, limits(4, all.bytes()))
        .unwrap();
    assert_eq!(exact, all);
    assert_eq!(
        service.control_work_page(&revision, None, limits(1, 1)),
        Err(ControlWorkError::Stop(StopWorkError::ByteLimit))
    );
    assert_eq!(pool.in_use(), 2);
    assert_eq!(
        service.home.as_ref().unwrap().home_revision().unwrap(),
        home_before
    );
    drop(permissions);
    let only_compaction = service.control_work_revision().unwrap();
    assert_eq!(
        service.control_work_page(&only_compaction, None, limits(1, 1)),
        Err(ControlWorkError::Compaction(CompactionWorkError::ByteLimit))
    );
    drop(continuations);
    assert_eq!(pool.in_use(), 0);
    let empty = service.control_work_revision().unwrap();
    let page = service
        .control_work_page(&empty, None, limits(1, 1))
        .unwrap();
    assert!(page.is_empty() && page.next_cursor().is_none());
    assert_eq!(all.len(), 4);
    let stop_only = permission(&service, 9);
    let stop_only_revision = service.control_work_revision().unwrap();
    let stop_only_page = service
        .control_work_page(&stop_only_revision, None, limits(1, 65_536))
        .unwrap();
    assert_eq!(stop_only_page.len(), 1);
    assert!(stop_only_page.next_cursor().is_none());
    assert_eq!(
        service
            .control_work_page(&stop_only_revision, None, limits(1, stop_only_page.bytes()))
            .unwrap(),
        stop_only_page
    );
    drop(stop_only);
    service.close().unwrap();
}

#[test]
fn composed_pages_revalidate_both_sources_across_every_cursor_stage() {
    let (_directory, service) = service();
    let permissions = [permission(&service, 1), permission(&service, 2)];
    let pool = &service.stop_coordinator.compaction_custody;
    let continuation = pool.reserve_continuation(thread(3), turn(3)).unwrap();
    for count in [1, 2, 256] {
        let revision = service.control_work_revision().unwrap();
        let result =
            service.collect_control_work_page(&revision, None, limits(count, 65_536), || {
                permissions[0].set_stage(PermissionInterruptionWorkStage::Driver);
            });
        assert_eq!(
            result,
            Err(ControlWorkError::Stop(StopWorkError::StaleRevision))
        );
        permissions[0].set_stage(PermissionInterruptionWorkStage::Reserved);
        let revision = service.control_work_revision().unwrap();
        let result =
            service.collect_control_work_page(&revision, None, limits(count, 65_536), || {
                let transient = pool.reserve_continuation(thread(4), turn(4)).unwrap();
                drop(transient);
            });
        assert_eq!(
            result,
            Err(ControlWorkError::Compaction(
                CompactionWorkError::StaleRevision
            ))
        );
    }
    let revision = service.control_work_revision().unwrap();
    let first = service
        .control_work_page(&revision, None, limits(2, 65_536))
        .unwrap();
    permissions[0].set_stage(PermissionInterruptionWorkStage::Driver);
    assert_eq!(
        service.control_work_page(&revision, first.next_cursor(), limits(2, 65_536)),
        Err(ControlWorkError::Stop(StopWorkError::StaleRevision))
    );
    let fresh = service.control_work_revision().unwrap();
    assert_eq!(
        service.control_work_page(&fresh, first.next_cursor(), limits(2, 65_536)),
        Err(ControlWorkError::ForeignCursor)
    );
    drop(permissions);
    drop(continuation);
    service.close().unwrap();
}

#[test]
fn composed_pages_reject_foreign_owners_and_unavailable_successors() {
    let (_directory, first) = service();
    let (_other_directory, second) = service();
    let permission = permission(&first, 1);
    let continuation = first
        .stop_coordinator
        .compaction_custody
        .reserve_continuation(thread(2), turn(2))
        .unwrap();
    let revision = first.control_work_revision().unwrap();
    let page = first
        .control_work_page(&revision, None, limits(1, 65_536))
        .unwrap();
    assert_eq!(
        second.validate_control_work_revision(&revision),
        Err(ControlWorkError::Stop(StopWorkError::ForeignRevision))
    );
    let foreign = second.control_work_revision().unwrap();
    assert_eq!(
        second.control_work_page(&foreign, page.next_cursor(), limits(1, 65_536)),
        Err(ControlWorkError::ForeignCursor)
    );
    assert_eq!(
        ControlWorkPageLimits::new(0, 1),
        Err(ControlWorkError::InvalidLimits)
    );
    assert_eq!(
        ControlWorkPageLimits::new(1, 0),
        Err(ControlWorkError::InvalidLimits)
    );
    assert_eq!(limits(usize::MAX, usize::MAX), limits(256, 65_536));
    let failed = first.collect_control_work_page(&revision, None, limits(1, 65_536), || {
        for seed in 10..91 {
            first
                .stop_coordinator
                .compaction_custody
                .source
                .begin(thread(seed), Some(turn(seed)));
        }
    });
    assert_eq!(
        failed,
        Err(ControlWorkError::Compaction(
            CompactionWorkError::RevisionUnavailable
        ))
    );
    assert_eq!(first.stop_coordinator.compaction_custody.in_use(), 1);
    drop(continuation);
    drop(permission);
    first.close().unwrap();
    second.close().unwrap();
}
