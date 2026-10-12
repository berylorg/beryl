use super::*;
use beryl_home_store::{CommandCancellation, CurrentDomainCommand};
use syndic_storage::indexed_resource as index;

#[path = "table_source.rs"]
mod table_source;

#[test]
fn indexed_source_pages_use_saved_raw_bytes_with_exact_root_and_release_custody() {
    let fixture = Fixture::new();
    let text = "aa".to_owned() + &"🍒".repeat(2048) + "xxx\n";
    let metadata = fixture.seed_resource(&text, 2, text.len() as u64, None, 7);
    complete_with_limits(&fixture, &metadata, 16384, 1);
    let cancel = AtomicBool::new(false);
    let mut request = indexed_request(&fixture, 7);
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    request.root = Some(prepared.response().root.as_ref().unwrap().digest);
    drop(prepared);
    let mut reconstructed = Vec::new();
    for range in [
        index::ByteRange {
            start: 2,
            end: 8192,
        },
        index::ByteRange {
            start: 8192,
            end: text.len() as u64,
        },
    ] {
        request.query = IndexedResourceQuery::Source {
            range,
            bytes: 16384,
        };
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel)
            .unwrap();
        assert!(prepared.response().encoded_bytes > (range.end - range.start) as usize);
        assert!(prepared.response().encoded_bytes <= 16384);
        fixture
            .reader
            .publish_indexed_resource_if_current(prepared, &request, &cancel, |response| {
                let IndexedResourceValue::Source(page) = &response.value else {
                    panic!("bound raw source page")
                };
                assert_eq!(page.binding.metadata, metadata);
                assert_eq!(page.range, range);
                assert_eq!(
                    page.bytes,
                    text.as_bytes()[range.start as usize..range.end as usize]
                );
                reconstructed.extend_from_slice(&page.bytes);
            })
            .unwrap();
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    }
    assert_eq!(reconstructed, text.as_bytes()[2..]);
    assert!(fixture.reader.retire());
}

#[test]
fn indexed_source_read_and_publication_refuse_invalid_or_obsolete_bindings() {
    for invalid in 0..5 {
        let fixture = Fixture::new();
        let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
        complete(&fixture, &metadata);
        let cancel = AtomicBool::new(false);
        let mut request = indexed_request(&fixture, 7);
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel)
            .unwrap();
        request.root = Some(prepared.response().root.as_ref().unwrap().digest);
        drop(prepared);
        request.query = IndexedResourceQuery::Source {
            range: index::ByteRange { start: 0, end: 4 },
            bytes: 16384,
        };
        if invalid < 3 {
            match invalid {
                0 => request.root = None,
                1 => request.root = Some([19; 32]),
                _ => {
                    request.query = IndexedResourceQuery::Source {
                        range: index::ByteRange { start: 0, end: 5 },
                        bytes: 16384,
                    }
                }
            }
            assert!(
                fixture
                    .reader
                    .prepare_indexed_resource(request, &cancel)
                    .is_err()
            );
        } else {
            let prepared = fixture
                .reader
                .prepare_indexed_resource(request.clone(), &cancel)
                .unwrap();
            if invalid == 3 {
                fixture.seed_head(2);
            } else {
                cancel.store(true, Ordering::Release);
            }
            let called = AtomicBool::new(false);
            assert!(
                fixture
                    .reader
                    .publish_indexed_resource_if_current(prepared, &request, &cancel, |_| called
                        .store(true, Ordering::Release))
                    .is_err()
            );
            assert!(!called.load(Ordering::Acquire));
        }
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        assert!(fixture.reader.retire());
    }
}

pub(super) fn indexed_request(
    fixture: &Fixture,
    revision: u64,
) -> TranscriptIndexedResourceRequest {
    TranscriptIndexedResourceRequest {
        resource: fixture.resource_request(2),
        revision: ProjectionRevision::new(revision).unwrap(),
        root: None,
        query: IndexedResourceQuery::Display {
            request: index::DisplayRequest {
                target: index::Target::Code(0),
                offset: 0,
            },
            cursor: None,
            bytes: 65_536,
        },
    }
}

fn current(fixture: &Fixture, command: CurrentDomainCommand) {
    let outcome = fixture.home.execute_current(command);
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

pub(super) fn complete(fixture: &Fixture, metadata: &ResourceMetadataRecord) {
    complete_with_limits(fixture, metadata, 3, 1);
}

pub(super) fn complete_with_limits(
    fixture: &Fixture,
    metadata: &ResourceMetadataRecord,
    bytes: usize,
    descriptors: usize,
) {
    let key = index::BuildKey {
        resource: index::ResourceKey {
            id: metadata.id(),
            revision: metadata.revision(),
        },
        operation: [63; 16],
    };
    current(
        fixture,
        fixture.storage.current_start_indexed_resource_build(key),
    );
    for _ in 0..100 {
        let frozen = fixture
            .home
            .capture_frozen_read(&CommandCancellation::new())
            .unwrap();
        let state = fixture
            .storage
            .read_frozen_indexed_resource_build(&fixture.home, &frozen, key)
            .unwrap()
            .unwrap();
        fixture.home.release_frozen_read(&frozen).unwrap();
        if state.status() == index::BuildStatus::Completed {
            return;
        }
        current(
            fixture,
            fixture
                .storage
                .current_advance_indexed_resource_build(state, bytes, descriptors)
                .unwrap(),
        );
    }
    panic!("bounded build did not complete");
}

#[test]
fn indexed_reads_transport_pending_range_display_and_anchor_without_retained_custody() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one💎\n", 0, 8, None, 7);
    let cancel = AtomicBool::new(false);
    let mut request = indexed_request(&fixture, 7);
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    assert!(matches!(
        prepared.response().value,
        IndexedResourceValue::Pending
    ));
    assert!(prepared.response().root.is_none());
    assert!(matches!(
        fixture
            .reader
            .prepare_resource(request.resource.clone(), &cancel),
        Err(TranscriptAttachmentError::Busy)
    ));
    assert_eq!(prepared.response().encoded_bytes, 1);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    fixture
        .reader
        .publish_indexed_resource_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    complete(&fixture, &metadata);
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    let root = prepared.response().root.as_ref().unwrap().digest;
    assert!(prepared.response().encoded_bytes <= 65_536);
    fixture
        .reader
        .publish_indexed_resource_if_current(prepared, &request, &cancel, |response| {
            let IndexedResourceValue::Display(display) = &response.value else {
                panic!("display")
            };
            assert_eq!(display.binding.metadata, metadata);
            assert_eq!(display.fragment.as_ref().unwrap().text, "one💎");
        })
        .unwrap();
    request.root = Some(root);
    request.query = IndexedResourceQuery::Range {
        request: index::Request::Code {
            lines: index::OrdinalRange { start: 0, end: 1 },
        },
        cursor: None,
        descriptors: 1,
        bytes: 65_536,
    };
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_indexed_resource_if_current(prepared, &request, &cancel, |response| {
            assert_eq!(response.root.as_ref().unwrap().digest, root);
            let IndexedResourceValue::Range(range) = &response.value else {
                panic!("range")
            };
            assert_eq!(range.items.len(), 1);
            assert!(range.request_complete);
        })
        .unwrap();
    let ResourceBacking::TextRange { source, range } = metadata.backing() else {
        panic!("source")
    };
    request.query = IndexedResourceQuery::Anchor {
        anchor: index::SourceAnchor {
            source,
            block_start: range.start(),
            kind: metadata.kind(),
            position: 2,
            visual_offset: -9,
        },
        bytes: 65_536,
    };
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_indexed_resource_if_current(prepared, &request, &cancel, |response| {
            let IndexedResourceValue::Anchor(anchor) = &response.value else {
                panic!("anchor")
            };
            assert_eq!(anchor.resolved.as_ref().unwrap().anchor.visual_offset, -9);
        })
        .unwrap();
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert!(fixture.reader.retire());
}

#[test]
fn indexed_completion_rejects_changed_inner_demand_and_mutation_after_preparation() {
    for changed in 0..6 {
        let fixture = Fixture::new();
        fixture.seed_resource("one\n", 0, 4, None, 7);
        let cancel = AtomicBool::new(false);
        let request = indexed_request(&fixture, 7);
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel)
            .unwrap();
        let mut expected = request.clone();
        let error = match changed {
            0 => {
                expected.root = Some([9; 32]);
                TranscriptAttachmentError::Identity
            }
            1 => {
                expected.revision = ProjectionRevision::new(8).unwrap();
                TranscriptAttachmentError::Identity
            }
            2 => {
                if let IndexedResourceQuery::Display { request, .. } = &mut expected.query {
                    request.offset = 1;
                }
                TranscriptAttachmentError::Identity
            }
            3 => {
                fixture.seed_head(2);
                TranscriptAttachmentError::Stale
            }
            4 => {
                cancel.store(true, Ordering::Release);
                TranscriptAttachmentError::Cancelled
            }
            _ => {
                assert!(!fixture.reader.retire());
                TranscriptAttachmentError::Retired
            }
        };
        let called = AtomicBool::new(false);
        assert_eq!(
            fixture.reader.publish_indexed_resource_if_current(
                prepared,
                &expected,
                &cancel,
                |_| called.store(true, Ordering::Release)
            ),
            Err(error)
        );
        assert!(!called.load(Ordering::Acquire));
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        assert!(fixture.reader.retire());
    }
}

#[test]
fn indexed_read_errors_release_worker_reservation_and_snapshot() {
    let fixture = Fixture::new();
    let metadata = fixture.seed_resource("one\n", 0, 4, None, 7);
    complete(&fixture, &metadata);
    let cancel = AtomicBool::new(false);
    for fault in 0..4 {
        let mut request = indexed_request(&fixture, 7);
        match fault {
            0 => request.revision = ProjectionRevision::new(8).unwrap(),
            1 => request.root = Some([19; 32]),
            2 => {
                if let IndexedResourceQuery::Display { bytes, .. } = &mut request.query {
                    *bytes = 1;
                }
            }
            _ => {
                if let IndexedResourceQuery::Display { bytes, .. } = &mut request.query {
                    *bytes = 65_537;
                }
            }
        }
        assert!(
            fixture
                .reader
                .prepare_indexed_resource(request, &cancel)
                .is_err()
        );
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        let valid = indexed_request(&fixture, 7);
        drop(
            fixture
                .reader
                .prepare_indexed_resource(valid, &cancel)
                .unwrap(),
        );
    }
    assert!(fixture.reader.retire());
}
