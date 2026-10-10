use super::*;
use crate::app_services::{PublishedRunningThreadsReader, RunningResourceConstruction};
use crate::cas_projection::{ResourceConstructionError, ResourceConstructionStep};
use crate::syndic_transcript::{
    DemandFact, DemandFactKind, ResidentTranscriptCore, ResidentTranscriptPolicy,
    TranscriptActivationSource,
};
use syndic_storage::indexed_resource as index;

fn published(fixture: &Fixture, lifetime: &Arc<()>) -> PublishedRunningThreadsReader {
    let (_provider, sessions) = crate::cas_projection::ProcessScheduledExecutionProvider::new();
    let attention = Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new());
    PublishedRunningThreadsReader::for_test(
        &fixture.service,
        &sessions,
        fixture.state.clone(),
        fixture.home.clone(),
        fixture.storage.clone(),
        &attention,
        lifetime,
    )
}

fn selected(fixture: &Fixture) -> (ResidentTranscriptCore, TranscriptResourceRequest) {
    let prepared = fixture
        .reader
        .prepare_attachment(fixture.request(1), &AtomicBool::new(false))
        .unwrap();
    let mut core = ResidentTranscriptCore::new(ResidentTranscriptPolicy::default());
    core.apply_prepared_activation(
        prepared.activation().clone(),
        TranscriptActivationSource::Test,
    );
    drop(prepared);
    core.push_demand_fact(DemandFact::new(
        core.presentation_snapshot().presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    let inner = indexed::indexed_request(fixture, 7);
    assert!(core.set_indexed_resource_demand(
        &core.resource_demand().unwrap(),
        Some(IndexedResourceDemand {
            revision: inner.revision,
            root: inner.root,
            query: inner.query,
        })
    ));
    let mut request = fixture.resource_request(2);
    request.demand = core.resource_demand().unwrap();
    (core, request)
}

#[test]
fn selected_worker_builds_missing_companion_in_bounded_steps_and_refreshes_pending() {
    for table in [false, true] {
        let fixture = Fixture::new();
        let (text, structure) = if table {
            (
                "a|b\n---|---\nx|y\n",
                ResourceStructure::table(1, 2, ProjectionSourceRange::new(0, 4).unwrap()).unwrap(),
            )
        } else {
            (
                "one💎\ntwo\n",
                ResourceStructure::code(Some("rust".into()), 2).unwrap(),
            )
        };
        let metadata =
            fixture.seed_structured_resource(text, 0, text.len() as u64, None, 7, structure);
        let lifetime = Arc::new(());
        let reader = published(&fixture, &lifetime);
        let (mut core, mut request) = selected(&fixture);
        if table {
            let mut inner = request.demand.indexed.clone().unwrap();
            inner.query = IndexedResourceQuery::Display {
                request: index::DisplayRequest {
                    target: index::Target::Cell {
                        role: index::RowRole::Body,
                        row: 0,
                        column: 0,
                    },
                    offset: 0,
                },
                cursor: None,
                bytes: 8192,
            };
            assert!(core.set_indexed_resource_demand(&request.demand, Some(inner)));
            request.demand = core.resource_demand().unwrap();
        }
        let prior = core.core_snapshot();
        let mut ready = false;
        let mut pending_count = 0;
        for _ in 0..100 {
            let worker = reader.clone();
            let provider = fixture.reader.clone();
            let expected = request.clone();
            let delivery = std::thread::spawn(move || {
                worker.prepare_running_resource(&provider, expected, &AtomicBool::new(false))
            })
            .join()
            .unwrap()
            .unwrap();
            assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
            assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
            let ResourceDeliveryResponse::Indexed(response) = delivery.prepared.response() else {
                panic!("indexed")
            };
            let pending = matches!(response.value, IndexedResourceValue::Pending);
            if let IndexedResourceValue::Display(display) = &response.value {
                assert_eq!(display.binding.metadata, metadata);
            }
            core = core
                .prepare_indexed_resource_response(&request.demand, response)
                .unwrap();
            fixture
                .reader
                .publish_resource_delivery_if_current(
                    delivery.prepared,
                    &request,
                    &AtomicBool::new(false),
                    |_| (),
                )
                .unwrap();
            assert_eq!(
                core.core_snapshot().resident.visible_range,
                prior.resident.visible_range
            );
            if !pending {
                ready = true;
                assert!(!core.refresh_pending_indexed_resource(&request.demand));
                break;
            }
            pending_count += 1;
            assert!(delivery.retry_pending);
            assert!(matches!(
                delivery.construction,
                Some(RunningResourceConstruction::Step(
                    ResourceConstructionStep::Command(CommandOutcome::Committed {
                        later_failure: None,
                        ..
                    })
                ))
            ));
            assert!(core.refresh_pending_indexed_resource(&request.demand));
            request.demand = core.resource_demand().unwrap();
        }
        assert!(ready && pending_count > 0);
        let revision = fixture.home.home_revision().unwrap();
        let cached = reader
            .prepare_running_resource(&fixture.reader, request, &AtomicBool::new(false))
            .unwrap();
        assert!(cached.construction.is_none());
        assert!(!cached.retry_pending);
        assert_eq!(fixture.home.home_revision().unwrap(), revision);
    }
}

#[test]
fn selected_worker_capacity_retry_and_retirement_do_not_write() {
    let fixture = Fixture::new();
    fixture.seed_resource("one\n", 0, 4, None, 7);
    let lifetime = Arc::new(());
    let reader = published(&fixture, &lifetime);
    let (_, request) = selected(&fixture);
    let revision = fixture.home.home_revision().unwrap();
    let release = fixture
        .service
        .saturate_scheduled_ordinary_capacity_for_test();
    let delivery = reader
        .prepare_running_resource(&fixture.reader, request.clone(), &AtomicBool::new(false))
        .unwrap();
    assert!(delivery.retry_pending);
    assert!(matches!(
        delivery.construction,
        Some(RunningResourceConstruction::Failed(
            ResourceConstructionError::Busy
        ))
    ));
    assert!(
        matches!(delivery.prepared.response(), ResourceDeliveryResponse::Indexed(response) if matches!(response.value, IndexedResourceValue::Pending))
    );
    drop(delivery);
    release();
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert!(matches!(
        reader.prepare_running_resource(&fixture.reader, request.clone(), &AtomicBool::new(true)),
        Err(TranscriptAttachmentError::Cancelled)
    ));
    drop(lifetime);
    assert!(matches!(
        reader.prepare_running_resource(&fixture.reader, request, &AtomicBool::new(false)),
        Err(TranscriptAttachmentError::Stale)
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn selected_worker_reconciles_original_indeterminate_command_before_fresh_read() {
    use beryl_home_store::{
        ReconciliationResolution,
        test_faults::{FaultController, FaultPoint},
    };
    let faults = FaultController::new();
    let fixture = Fixture::with_faults(faults.clone());
    fixture.seed_resource("one\n", 0, 4, None, 7);
    let lifetime = Arc::new(());
    let reader = published(&fixture, &lifetime);
    let (_, request) = selected(&fixture);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let delivery = reader
        .prepare_running_resource(&fixture.reader, request.clone(), &AtomicBool::new(false))
        .unwrap();
    assert!(delivery.retry_pending);
    let Some(RunningResourceConstruction::Reconciled {
        failure,
        handle,
        resolution,
    }) = delivery.construction
    else {
        panic!("reconciliation evidence")
    };
    assert!(matches!(
        resolution,
        Ok(ReconciliationResolution::ExactNew { .. })
    ));
    assert!(!failure.to_string().is_empty());
    assert_eq!(
        fixture.home.reconcile(&handle).unwrap(),
        resolution.unwrap()
    );
    assert!(fixture.home.pending_reconciliations().is_empty());
    assert!(
        matches!(delivery.prepared.response(), ResourceDeliveryResponse::Indexed(response) if matches!(response.value, IndexedResourceValue::Pending))
    );
    fixture
        .reader
        .publish_resource_delivery_if_current(
            delivery.prepared,
            &request,
            &AtomicBool::new(false),
            |_| (),
        )
        .unwrap();
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}
