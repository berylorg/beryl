use super::*;
use beryl_app::cas_projection::{
    CompactionCommandWorkStage, CompactionWorkPage, CompactionWorkPageLimits, ContinuationWorkStage,
};

fn page(fixture: &LifecycleFixture) -> CompactionWorkPage {
    let revision = fixture.service.compaction_work_revision().unwrap();
    let page = fixture
        .service
        .compaction_work_page(
            &revision,
            None,
            CompactionWorkPageLimits::new(80, 65_536).unwrap(),
        )
        .unwrap();
    let control_revision = fixture.service.control_work_revision().unwrap();
    let control = fixture
        .service
        .control_work_page(
            &control_revision,
            None,
            beryl_app::cas_projection::ControlWorkPageLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(control.compaction_records(), page.records());
    page
}

#[test]
fn terminal_compaction_work_survives_local_removal_until_driver_disposal_without_waiter_custody() {
    let fixture = LifecycleFixture::new(245, 180);
    let waiter = fixture.harness.retain_compaction_result(fixture.thread_id);
    let initial = page(&fixture);
    assert_eq!(initial.records().len(), 1);
    assert_eq!(
        initial.records()[0].continuation.as_ref().unwrap().stage,
        ContinuationWorkStage::Registered
    );
    assert_eq!(
        initial.records()[0].compaction.as_ref().unwrap().command,
        Some(CompactionCommandWorkStage::Driver)
    );
    fixture
        .harness
        .observe_response(
            fixture.operation_id,
            fixture.operation().attempt(),
            CompactionRequestDisposition::Accepted,
        )
        .unwrap();
    let response = page(&fixture);
    assert_eq!(
        response.records()[0]
            .compaction
            .as_ref()
            .unwrap()
            .request_disposition,
        Some(CompactionRequestDisposition::Accepted)
    );
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let cleanup = page(&fixture);
    assert_eq!(cleanup.records().len(), 1);
    assert!(cleanup.records()[0].continuation.is_none());
    let compaction = cleanup.records()[0].compaction.as_ref().unwrap();
    assert!(!compaction.local_registered);
    assert_eq!(
        compaction.command,
        Some(CompactionCommandWorkStage::Cleanup)
    );
    assert_eq!(compaction.result, Some(ContextCompactionOutcome::Succeeded));
    assert_eq!(fixture.harness.compaction_custody_in_use(), 1);
    fixture.harness.release_compaction_driver();
    assert_eq!(waiter.wait(), ContextCompactionOutcome::Succeeded);
    assert!(page(&fixture).records().is_empty());
    assert_eq!(fixture.harness.compaction_custody_in_use(), 0);
    assert_eq!(cleanup.records().len(), 1);
    fixture.close();
    assert_eq!(waiter.wait(), ContextCompactionOutcome::Succeeded);
}
