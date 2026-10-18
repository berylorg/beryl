use super::*;
use crate::cas_projection::{ResourceConstructionError, ResourceConstructionStep};
use syndic_storage::indexed_resource as index;

fn build(metadata: &ResourceMetadataRecord) -> index::BuildKey {
    index::BuildKey {
        resource: index::ResourceKey {
            id: metadata.id(),
            revision: metadata.revision(),
        },
        operation: [91; 16],
    }
}

#[test]
fn competing_missing_and_building_frontiers_preserve_refusal_and_report_peer_progress() {
    let fixture = Fixture::new();
    let text = "x\n".repeat(200);
    let metadata = fixture.seed_structured_resource(
        &text,
        0,
        text.len() as u64,
        None,
        7,
        ResourceStructure::code(None, 200).unwrap(),
    );
    let key = build(&metadata);
    for _ in 0..2 {
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let (sent, received) = std::sync::mpsc::channel();
        let first = fixture.service.resource_construction_authority();
        let second = first.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            let result = first
                .step_after_observation(key, &AtomicBool::new(false), || {
                    first_barrier.wait();
                })
                .unwrap();
            sent.send(()).unwrap();
            result
        });
        let second = std::thread::spawn(move || {
            second
                .step_after_observation(key, &AtomicBool::new(false), || {
                    barrier.wait();
                    received
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                })
                .unwrap()
        });
        assert!(matches!(
            first.join().unwrap(),
            ResourceConstructionStep::Command(CommandOutcome::Committed {
                later_failure: None,
                ..
            })
        ));
        assert!(matches!(
            second.join().unwrap(),
            ResourceConstructionStep::PeerProgress(CommandOutcome::NotCommitted { .. })
        ));
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    }
}

#[test]
fn service_steps_missing_companions_to_ready_without_changing_history() {
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
        let key = build(&metadata);
        let cancelled = AtomicBool::new(false);
        let mut completed = false;
        for _ in 0..100 {
            match fixture
                .service
                .step_resource_construction(key, &cancelled)
                .unwrap()
            {
                ResourceConstructionStep::Command(outcome)
                | ResourceConstructionStep::PeerProgress(outcome) => assert!(
                    matches!(
                        outcome,
                        CommandOutcome::Committed {
                            later_failure: None,
                            ..
                        }
                    ),
                    "{outcome:?}"
                ),
                ResourceConstructionStep::Terminal(index::BuildStatus::Completed) => {
                    completed = true;
                    break;
                }
                ResourceConstructionStep::Terminal(status) => panic!("unexpected {status:?}"),
            }
            assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
            assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
        }
        assert!(completed);
        let revision = fixture.home.home_revision().unwrap();
        assert!(matches!(
            fixture
                .service
                .step_resource_construction(key, &cancelled)
                .unwrap(),
            ResourceConstructionStep::Terminal(index::BuildStatus::Completed)
        ));
        assert_eq!(fixture.home.home_revision().unwrap(), revision);
        let mut request = indexed::indexed_request(&fixture, 7);
        if table {
            request.resource.demand.kind = ResourceKind::Table;
            request.query = IndexedResourceQuery::Display {
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
        }
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancelled)
            .unwrap();
        assert!(matches!(
            prepared.response().value,
            IndexedResourceValue::Display(_)
        ));
        fixture
            .reader
            .publish_indexed_resource_if_current(&prepared, &request, &cancelled, |response| {
                let IndexedResourceValue::Display(display) = &response.value else {
                    panic!("display")
                };
                assert_eq!(display.binding.metadata, metadata);
            })
            .unwrap();
        drop(prepared);
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    }
}

#[test]
fn cancelled_or_capacity_refused_steps_do_not_admit_a_build() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let key = build(&metadata);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        fixture.service.step_resource_construction(key, &cancelled),
        Err(ResourceConstructionError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    let release = fixture
        .service
        .saturate_scheduled_ordinary_capacity_for_test();
    assert!(matches!(
        fixture.service.step_resource_construction(key, &cancelled),
        Err(ResourceConstructionError::Busy)
    ));
    release();
    let frozen = fixture
        .home
        .capture_frozen_read(&beryl_home_store::CommandCancellation::new())
        .unwrap();
    assert!(
        fixture
            .storage
            .read_frozen_indexed_resource_build(&fixture.home, &frozen, key)
            .unwrap()
            .is_none()
    );
    fixture.home.release_frozen_read(&frozen).unwrap();
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn stale_resource_admission_returns_the_original_not_committed_outcome() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let mut key = build(&metadata);
    key.resource.revision = ProjectionRevision::new(8).unwrap();
    let revision = fixture.home.home_revision().unwrap();
    assert!(matches!(
        fixture
            .service
            .step_resource_construction(key, &AtomicBool::new(false))
            .unwrap(),
        ResourceConstructionStep::Command(CommandOutcome::NotCommitted { .. })
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn cloned_background_authority_shares_service_capacity_and_frontier() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let key = build(&metadata);
    let authority = fixture.service.resource_construction_authority();
    let clone = authority.clone();
    let release = fixture
        .service
        .saturate_scheduled_ordinary_capacity_for_test();
    assert!(matches!(
        std::thread::spawn(move || clone.step(key, &AtomicBool::new(false)))
            .join()
            .unwrap(),
        Err(ResourceConstructionError::Busy)
    ));
    release();
    std::thread::spawn(move || {
        for _ in 0..100 {
            match authority.step(key, &AtomicBool::new(false)).unwrap() {
                ResourceConstructionStep::Command(CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }) => {}
                ResourceConstructionStep::Terminal(index::BuildStatus::Completed) => return,
                _ => panic!("construction failed"),
            }
        }
        panic!("construction did not complete");
    })
    .join()
    .unwrap();
    assert!(matches!(
        fixture
            .service
            .step_resource_construction(key, &AtomicBool::new(false))
            .unwrap(),
        ResourceConstructionStep::Terminal(index::BuildStatus::Completed)
    ));
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}

#[test]
fn retained_background_authority_rejects_closed_service() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let key = build(&metadata);
    let authority = fixture.service.resource_construction_authority();
    assert!(fixture.service.close().is_ok());
    assert!(matches!(
        std::thread::spawn(move || authority.step(key, &AtomicBool::new(false)))
            .join()
            .unwrap(),
        Err(ResourceConstructionError::Admission(
            crate::cas_projection::LiveCommandAdmissionError::Closed
        ))
    ));
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn published_reader_constructs_on_background_thread_and_rejects_retired_publication() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    let key = build(&metadata);
    let (_provider, sessions) = crate::cas_projection::ProcessScheduledExecutionProvider::new();
    let attention = Arc::new(crate::lifecycle_attention::ProcessLifecycleAttentionPool::new());
    let lifetime = Arc::new(());
    let reader = crate::app_services::PublishedRunningThreadsReader::for_test(
        &fixture.service,
        &sessions,
        fixture.state.clone(),
        fixture.home.clone(),
        fixture.storage.clone(),
        &attention,
        &lifetime,
    );
    let worker = reader.clone();
    assert!(matches!(
        std::thread::spawn(move || worker.step_resource_construction(key, &AtomicBool::new(false)))
            .join()
            .unwrap()
            .unwrap(),
        ResourceConstructionStep::Command(CommandOutcome::Committed {
            later_failure: None,
            ..
        })
    ));
    let revision = fixture.home.home_revision().unwrap();
    drop(lifetime);
    assert!(matches!(
        reader.step_resource_construction(key, &AtomicBool::new(false)),
        Err(ResourceConstructionError::Unavailable)
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
}
