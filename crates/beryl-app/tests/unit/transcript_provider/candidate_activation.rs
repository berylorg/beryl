use super::*;
use crate::syndic_transcript::{
    ProjectionPayload, TranscriptNarrativeKind, TranscriptProviderHistoryReason,
    TranscriptProviderHistoryState, TranscriptProviderResponseKind,
};
use beryl_home_store::{
    CommandCancellation, HomeOpenPublication,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{SyndicItemId as StoredItemId, SyndicProjectionId, SyndicResourceId};

struct CandidateFixture {
    publication: HomeOpenPublication,
    storage: SyndicStorage,
    old_storage: SyndicStorage,
    faults: FaultController,
    _directory: tempfile::TempDir,
}

fn into_candidate(fixture: Fixture) -> CandidateFixture {
    let Fixture {
        service,
        reader,
        home,
        storage,
        state,
        graph,
        host,
        _directory,
    } = fixture;
    drop((reader, home, state, graph, host));
    drop(service);
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(_directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    BerylState::register(&mut candidate).unwrap();
    let fresh = SyndicStorage::register(&mut candidate).unwrap();
    let publication = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    CandidateFixture {
        _directory,
        publication,
        storage: fresh,
        old_storage: storage,
        faults,
    }
}

fn ordinary_activation(fixture: &Fixture) -> PreparedTranscriptActivation {
    fixture
        .reader
        .prepare_attachment(fixture.request(1), &AtomicBool::new(false))
        .unwrap()
        .activation
        .clone()
}

fn seed_resource(fixture: &Fixture) {
    use syndic_storage::{
        MarkdownBlockId, MarkdownBlockKind, ProjectionOrdinal, ProjectionRecord,
        ProjectionSourceRange, ResourceBacking, ResourceKind, ResourceMetadataRecord,
        ResourceOrdinal, ResourceStructure,
    };
    let item_id = StoredItemId::from_bytes([11; 16]);
    let projection_id = SyndicProjectionId::from_bytes([13; 16]);
    let resource_id = SyndicResourceId::from_bytes([20; 16]);
    let item = fixture
        .storage
        .canonical_item(
            &fixture.home,
            item_id,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let projection = fixture
        .storage
        .projection(
            &fixture.home,
            projection_id,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let range = ProjectionSourceRange::new(0, 8).unwrap();
    let resource = ResourceMetadataRecord::new(
        resource_id,
        ProjectionRevision::new(1).unwrap(),
        projection_id,
        item_id,
        ResourceOrdinal::FIRST,
        ResourceKind::Attachment,
        "text/plain",
        ResourceBacking::TextRange {
            source: item.projection_source().unwrap(),
            range,
        },
        [22; 32],
        Some(range),
        ResourceStructure::Opaque,
    )
    .unwrap();
    let replacement = ProjectionRecord::new(
        projection_id,
        projection.revision(),
        item_id,
        projection.turn_id(),
        ProjectionOrdinal::FIRST,
        syndic_storage::ProjectionPayload::resource_reference(
            MarkdownBlockId::from_bytes([21; 32]),
            MarkdownBlockKind::FencedCode,
            range,
            resource_id,
            "resource",
        )
        .unwrap(),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Resource(resource)).unwrap();
    batch.put(FixtureRecord::Projection(replacement)).unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
}

#[test]
fn populated_candidate_activation_preserves_text_resource_narrative_and_ordinary_decoding() {
    for resource in [false, true] {
        let fixture = Fixture::new();
        fixture.seed_projection("authored", 1);
        if resource {
            seed_resource(&fixture);
        }
        let expected = ordinary_activation(&fixture);
        let mut candidate = into_candidate(fixture);
        let access = candidate.publication.recovery_access().unwrap();
        let before = access.home_revision().unwrap();
        let prepared = prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &CommandCancellation::new(),
        )
        .unwrap();
        assert_eq!(prepared, expected);
        assert_eq!(access.home_revision().unwrap(), before);
        let Some(TranscriptProviderResponseKind::ViewPage(page)) = prepared.view_page_response
        else {
            panic!("missing populated candidate view");
        };
        assert_eq!(page.records.len(), 1);
        assert_eq!(
            page.records[0].narrative_kind,
            TranscriptNarrativeKind::UserInput
        );
        assert_eq!(page.records[0].position.0, 1);
        let Some(TranscriptProviderResponseKind::ProjectionRecords(projections)) =
            prepared.projection_records_response
        else {
            panic!("missing populated candidate projections");
        };
        assert_eq!(projections.records.len(), 1);
        if resource {
            assert!(
                matches!(&projections.records[0].payload, ProjectionPayload::ResourceReference { label: Some(label), .. } if label.as_ref() == "resource")
            );
        } else {
            assert!(
                matches!(&projections.records[0].payload, ProjectionPayload::Text { text } if text.as_ref() == "authored")
            );
        }
        candidate.publication.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_activation_preserves_stale_unpublished_and_local_capacity_results() {
    let fixture = Fixture::new();
    fixture.seed_projection("stale", 2);
    assert!(matches!(
        fixture
            .reader
            .prepare_attachment(fixture.request(1), &AtomicBool::new(false)),
        Err(TranscriptAttachmentError::Stale)
    ));
    let mut candidate = into_candidate(fixture);
    let access = candidate.publication.recovery_access().unwrap();
    assert_eq!(
        prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &CommandCancellation::new()
        )
        .unwrap_err(),
        TranscriptAttachmentError::Stale.to_string()
    );
    candidate.publication.publish().unwrap().close().unwrap();
    for capacity in [false, true] {
        let fixture = Fixture::new();
        if capacity {
            fixture.seed_projection(&"x".repeat(ATTACHMENT_MAX_BYTES), 1);
        } else {
            fixture.seed_head_lifecycle(2, ProjectionLifecycle::Stale);
        }
        let expected = ordinary_activation(&fixture);
        let mut candidate = into_candidate(fixture);
        let access = candidate.publication.recovery_access().unwrap();
        let prepared = prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &CommandCancellation::new(),
        )
        .unwrap();
        assert_eq!(prepared, expected);
        let Some(TranscriptProviderResponseKind::ViewPage(page)) = prepared.view_page_response
        else {
            panic!("missing local candidate state");
        };
        assert!(page.records.is_empty());
        let expected_reason = if capacity {
            TranscriptProviderHistoryReason::PresentationCapacity
        } else {
            TranscriptProviderHistoryReason::ProjectionStale
        };
        assert!(
            matches!(page.history_state, TranscriptProviderHistoryState::Unavailable { reason, .. } if reason == expected_reason)
        );
        candidate.publication.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_activation_rejects_old_foreign_and_recovered_generation_sources() {
    let fixture = Fixture::new();
    fixture.seed_projection("authored", 1);
    let expected = ordinary_activation(&fixture);
    let mut candidate = into_candidate(fixture);
    let foreign = Fixture::new();
    let access = candidate.publication.recovery_access().unwrap();
    for storage in [&candidate.old_storage, &foreign.storage] {
        assert!(
            prepare_candidate_activation(
                &access,
                storage,
                WindowId::from_bytes([5; 16]),
                SyndicThreadId::from_bytes([1; 16]),
                &CommandCancellation::new()
            )
            .is_err()
        );
    }
    assert!(
        prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([250; 16]),
            &CommandCancellation::new()
        )
        .is_err()
    );
    let store = candidate.publication.publish().unwrap();
    candidate
        .faults
        .fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
    let access = recovered.recovery_access().unwrap();
    assert!(
        prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &CommandCancellation::new()
        )
        .is_err()
    );
    assert_eq!(
        prepare_candidate_activation(
            &access,
            &fresh,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &CommandCancellation::new()
        )
        .unwrap(),
        expected
    );
    recovered.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_activation_keeps_the_bounded_tail_and_exact_projection_byte_budget() {
    use syndic_storage::{ItemProjectionGeneration, TranscriptPosition, TranscriptViewEntryRecord};
    let fixture = Fixture::new();
    fixture.seed_projection("authored", 1);
    let thread_id = fixture.request(1).thread_id;
    let thread = fixture
        .storage
        .thread(
            &fixture.home,
            thread_id,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    for position in 1..=70 {
        batch
            .put(FixtureRecord::TranscriptViewEntry(
                TranscriptViewEntryRecord::new(
                    thread_id,
                    TranscriptGeneration::FIRST,
                    TranscriptPosition::new(position).unwrap(),
                    StoredItemId::from_bytes([11; 16]),
                    ProjectionRevision::new(1).unwrap(),
                    ItemProjectionGeneration::FIRST,
                    SyndicProjectionId::from_bytes([13; 16]),
                    ProjectionRevision::new(1).unwrap(),
                ),
            ))
            .unwrap();
    }
    batch
        .put(FixtureRecord::TranscriptViewHead(
            TranscriptViewHeadRecord::new(
                thread_id,
                TranscriptGeneration::FIRST,
                ProjectionRevision::new(1).unwrap(),
                70,
                thread.committed_tail(),
                thread.selected_path_digest(),
                ProjectionLifecycle::Current,
            ),
        ))
        .unwrap();
    execute(
        &fixture.home,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.home).unwrap(), batch),
    );
    let expected = ordinary_activation(&fixture);
    let mut candidate = into_candidate(fixture);
    let access = candidate.publication.recovery_access().unwrap();
    let prepared = prepare_candidate_activation(
        &access,
        &candidate.storage,
        WindowId::from_bytes([5; 16]),
        thread_id,
        &CommandCancellation::new(),
    )
    .unwrap();
    assert_eq!(prepared, expected);
    let Some(TranscriptProviderResponseKind::ViewPage(page)) = prepared.view_page_response else {
        panic!("missing bounded candidate tail");
    };
    assert!(!page.at_start);
    assert!(page.at_end);
    assert!(!page.records.is_empty());
    assert!(page.records.len() <= ATTACHMENT_MAX_RECORDS);
    assert_eq!(page.records.last().unwrap().position.0, 70);
    assert_eq!(
        page.records.first().unwrap().position.0,
        71 - page.records.len() as u64
    );
    assert!(page.records.len() * ("authored".len() + 1024) <= ATTACHMENT_MAX_BYTES);
    candidate.publication.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_activation_cancellation_stops_before_the_next_bounded_read() {
    let fixture = Fixture::new();
    fixture.seed_projection("authored", 1);
    let mut candidate = into_candidate(fixture);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let access = candidate.publication.recovery_access().unwrap();
    assert!(
        prepare_candidate_activation(
            &access,
            &candidate.storage,
            WindowId::from_bytes([5; 16]),
            SyndicThreadId::from_bytes([1; 16]),
            &cancellation
        )
        .unwrap_err()
        .contains("cancelled")
    );
    let cancellation = CommandCancellation::new();
    let blocks: Vec<_> = (0..3)
        .map(|_| {
            candidate
                .faults
                .block_next(FaultPoint::BeforeReadConfirmation)
        })
        .collect();
    let faults = candidate.faults.clone();
    let worker_cancellation = cancellation.clone();
    let worker = std::thread::spawn(move || {
        for (index, block) in blocks.iter().enumerate() {
            if !block.wait_until_reached(std::time::Duration::from_secs(5)) {
                for pending in &blocks {
                    pending.release();
                }
                panic!("candidate read did not reach its bounded cancellation cut");
            }
            if index == 2 {
                worker_cancellation.cancel();
                faults.fail_next(FaultPoint::BeforeReadConfirmation);
            }
            block.release();
        }
    });
    let result = prepare_candidate_activation(
        &access,
        &candidate.storage,
        WindowId::from_bytes([5; 16]),
        SyndicThreadId::from_bytes([1; 16]),
        &cancellation,
    );
    worker.join().unwrap();
    assert_eq!(
        result.unwrap_err(),
        TranscriptAttachmentError::Cancelled.to_string()
    );
    assert!(
        access.home_revision().is_err(),
        "the following read fault must remain unconsumed"
    );
}
