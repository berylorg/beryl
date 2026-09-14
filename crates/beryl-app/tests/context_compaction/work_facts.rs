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
fn shutdown_capture_retains_exact_compaction_receipt_after_successor_and_source_release() {
    let fixture = LifecycleFixture::new(246, 181);
    fixture.publish_success_prefix();
    let pause = fixture.harness.pause_after_lifecycle_settlement().unwrap();
    let (mut capture, fence) = thread::scope(|scope| {
        let terminal = scope.spawn(|| fixture.publish_success_terminal());
        pause.wait_until_settled();
        assert_ne!(
            fixture.committed_tail(),
            Some(fixture.operation_id.provider_turn_id())
        );
        let fence = fixture.process_admission.test_fence().unwrap();
        let mut capture =
            beryl_app::cas_projection::test_faults::ShutdownExecutionCaptureProbe::new(
                &fixture.service,
                &fence,
            )
            .unwrap();
        capture.refresh(&fixture.service).unwrap();
        assert_eq!(capture.counts(), (0, 1));
        assert_eq!(
            capture
                .compaction_settled(&fixture.service, fixture.operation_id)
                .unwrap(),
            Some(true)
        );
        pause.release();
        terminal.join().unwrap();
        (capture, fence)
    });
    let pending = fixture.committed_tail();
    fixture.harness.release_compaction_driver();
    assert!(page(&fixture).records().is_empty());
    assert_eq!(fixture.harness.compaction_custody_in_use(), 0);
    capture.refresh(&fixture.service).unwrap();
    assert_eq!(capture.counts(), (0, 1));
    assert_eq!(
        capture
            .compaction_settled(&fixture.service, fixture.operation_id)
            .unwrap(),
        Some(true)
    );
    assert_eq!(fixture.committed_tail(), pending);
    let foreign = LifecycleFixture::new(247, 182);
    assert!(
        capture
            .compaction_settled(&foreign.service, fixture.operation_id)
            .is_err()
    );
    foreign.close();
    drop(capture);
    fence.try_reopen(true).unwrap();
    fixture.close();
}

#[test]
fn terminal_compaction_work_survives_local_removal_until_driver_disposal_without_waiter_custody() {
    let (fixture, sessions) = LifecycleFixture::with_process_sessions(245, 180);
    let attention = beryl_app::lifecycle_attention::ProcessLifecycleAttentionPool::new();
    let inventory_page = || {
        let inventory = fixture
            .service
            .process_work_inventory(&sessions, &attention);
        let revision = inventory.revision().unwrap();
        inventory
            .page(
                &revision,
                None,
                beryl_app::cas_projection::ProcessWorkPageLimits::new(256, 65_536).unwrap(),
                &beryl_app::cas_projection::ProjectionCancellationToken::new(),
            )
            .unwrap()
    };
    let waiter = fixture.harness.retain_compaction_result(fixture.thread_id);
    let initial = page(&fixture);
    let running = inventory_page();
    assert_eq!(running.total_threads(), 1);
    assert!(running.records()[0].facts.compacting && running.records()[0].facts.continuation);
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
    let running = inventory_page();
    assert_eq!(running.total_threads(), 1);
    assert!(running.records()[0].facts.compacting && running.records()[0].facts.cleanup);
    assert!(!running.records()[0].facts.continuation);
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
    let successor = inventory_page();
    assert_eq!(successor.total_threads(), 1);
    assert!(!successor.records()[0].facts.compacting && !successor.records()[0].facts.cleanup);
    assert!(successor.records()[0].facts.pending || successor.records()[0].facts.queued);
    assert_eq!(fixture.harness.compaction_custody_in_use(), 0);
    assert_eq!(cleanup.records().len(), 1);
    fixture.close();
    assert_eq!(waiter.wait(), ContextCompactionOutcome::Succeeded);
}
