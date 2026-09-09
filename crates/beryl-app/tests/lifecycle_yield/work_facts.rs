use super::*;
use beryl_app::cas_projection::{
    CompactionWorkError, CompactionWorkPage, CompactionWorkPageLimits,
};

pub(super) fn page(fixture: &Fixture) -> CompactionWorkPage {
    let deadline = std::time::Instant::now() + server::TIMEOUT;
    loop {
        let revision = fixture.store.compaction_work_revision().unwrap();
        match fixture.store.compaction_work_page(
            &revision,
            None,
            CompactionWorkPageLimits::new(256, 65_536).unwrap(),
        ) {
            Ok(page) => return page,
            Err(CompactionWorkError::StaleRevision) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "compaction work did not stabilize"
                );
                thread::yield_now();
            }
            other => panic!("compaction work query failed: {other:?}"),
        }
    }
}

#[test]
fn work_queries_reject_foreign_service_identity_without_admitting_execution() {
    let first = Fixture::new(243);
    let second = Fixture::new(244);
    let revision = first.store.compaction_work_revision().unwrap();
    let limits = CompactionWorkPageLimits::new(1, 65_536).unwrap();
    assert_eq!(first.store.compaction_work_revision().unwrap(), revision);
    assert!(
        first
            .store
            .compaction_work_page(&revision, None, limits)
            .unwrap()
            .records()
            .is_empty()
    );
    assert_eq!(
        second.store.validate_compaction_work_revision(&revision),
        Err(CompactionWorkError::ForeignRevision)
    );
    assert_eq!(
        second.store.compaction_work_page(&revision, None, limits),
        Err(CompactionWorkError::ForeignRevision)
    );
    assert_eq!(first.store.worker_pool_diagnostics().active(), 0);
    assert_eq!(second.store.worker_pool_diagnostics().active(), 0);
    assert_eq!(
        CompactionWorkPageLimits::new(0, 1),
        Err(CompactionWorkError::InvalidLimits)
    );
    assert_eq!(
        CompactionWorkPageLimits::new(1, 0),
        Err(CompactionWorkError::InvalidLimits)
    );
    let (first_directory, first_service) = first.into_service();
    let (second_directory, second_service) = second.into_service();
    first_service.close().unwrap();
    second_service.close().unwrap();
    drop(first_directory);
    drop(second_directory);
}
