use super::*;

fn seed_table(fixture: &Fixture, body: &str, rows: u64) -> (String, ResourceMetadataRecord) {
    let prefix = "prefix\n";
    let header = " a | b | c \r\n";
    let table = format!("{header}---|---:|:---:\r\n{body}");
    let text = format!("{prefix}{table}suffix\n");
    let start = prefix.len() as u64;
    let end = start + table.len() as u64;
    let metadata = fixture.seed_structured_resource(
        &text,
        start,
        end,
        None,
        7,
        ResourceStructure::table(
            rows,
            3,
            ProjectionSourceRange::new(0, header.len() as u64).unwrap(),
        )
        .unwrap(),
    );
    (text, metadata)
}

fn table_request(fixture: &Fixture) -> TranscriptIndexedResourceRequest {
    let mut request = indexed_request(fixture, 7);
    request.resource.demand.kind = ResourceKind::Table;
    request.query = IndexedResourceQuery::Range {
        request: index::Request::Table {
            role: index::RowRole::Header,
            rows: index::OrdinalRange { start: 0, end: 1 },
            columns: index::OrdinalRange { start: 0, end: 3 },
        },
        cursor: None,
        descriptors: 3,
        bytes: 8192,
    };
    request
}

fn source_request(
    fixture: &Fixture,
    metadata: &ResourceMetadataRecord,
) -> TranscriptIndexedResourceRequest {
    let cancel = AtomicBool::new(false);
    let mut request = table_request(fixture);
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    request.root = Some(prepared.response().root.as_ref().unwrap().digest);
    drop(prepared);
    let ResourceBacking::TextRange { range, .. } = metadata.backing() else {
        panic!("table source")
    };
    request.query = IndexedResourceQuery::Source {
        range: index::ByteRange {
            start: range.start(),
            end: range.end().min(range.start() + 8192),
        },
        bytes: 16384,
    };
    request
}

#[test]
fn table_source_pages_preserve_authored_syntax_and_contiguous_split_scalars() {
    let fixture = Fixture::new();
    let body = format!(
        "  x\\|y | **z** ||\r\n y | z \n a|b|c|surplus\r\nq{}|tail|last\r\n",
        "🍒".repeat(2048)
    );
    let (text, metadata) = seed_table(&fixture, &body, 4);
    complete_with_limits(&fixture, &metadata, 16384, 64);
    let cancel = AtomicBool::new(false);
    let mut request = source_request(&fixture, &metadata);
    let ResourceBacking::TextRange { source, range } = metadata.backing() else {
        panic!("table source")
    };
    let original_root = request.root;
    let mut reconstructed = Vec::new();
    let mut offset = range.start();
    while offset < range.end() {
        let end = (offset + 8192).min(range.end());
        request.query = IndexedResourceQuery::Source {
            range: index::ByteRange { start: offset, end },
            bytes: 16384,
        };
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel)
            .unwrap();
        assert_eq!(
            prepared.response().root.as_ref().map(|root| root.digest),
            original_root
        );
        assert!(prepared.response().encoded_bytes > (end - offset) as usize);
        assert!(prepared.response().encoded_bytes <= 16384);
        fixture
            .reader
            .publish_indexed_resource_if_current(&prepared, &request, &cancel, |response| {
                let IndexedResourceValue::Source(page) = &response.value else {
                    panic!("exact table source page")
                };
                assert_eq!(page.binding.metadata, metadata);
                assert_eq!(page.binding.source, source);
                assert_eq!(page.binding.range.start, range.start());
                assert_eq!(page.binding.range.end, range.end());
                assert_eq!(page.range, index::ByteRange { start: offset, end });
                assert_eq!(page.bytes, text.as_bytes()[offset as usize..end as usize]);
                assert!(page.bytes.len() <= 8192);
                if offset == range.start() {
                    assert_eq!(page.bytes.len(), 8192);
                    assert!(std::str::from_utf8(&page.bytes).is_err());
                }
                reconstructed.extend_from_slice(&page.bytes);
            })
            .unwrap();
        drop(prepared);
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        offset = end;
    }
    assert_eq!(
        reconstructed,
        text.as_bytes()[range.start() as usize..range.end() as usize]
    );
    assert!(std::str::from_utf8(&reconstructed).is_ok());
    assert!(fixture.reader.retire());
}

#[test]
fn table_source_refuses_root_kind_revision_and_page_boundary_mismatches() {
    let fixture = Fixture::new();
    let body = format!("{}|z|last\n", "x".repeat(8192));
    let (_, metadata) = seed_table(&fixture, &body, 1);
    complete_with_limits(&fixture, &metadata, 16384, 64);
    let valid = source_request(&fixture, &metadata);
    let ResourceBacking::TextRange { range, .. } = metadata.backing() else {
        panic!("table source")
    };
    let cancel = AtomicBool::new(false);
    for fault in 0..10 {
        let mut request = valid.clone();
        match fault {
            0 => request.root = None,
            1 => request.root = Some([19; 32]),
            2 => request.resource.demand.kind = ResourceKind::Code,
            3 => request.revision = ProjectionRevision::new(8).unwrap(),
            _ => {
                let (start, end, bytes) = match fault {
                    4 => (range.start(), range.start(), 16384),
                    5 => (range.start() + 1, range.start(), 16384),
                    6 => (range.start() - 1, range.start() + 1, 16384),
                    7 => (range.end() - 1, range.end() + 1, 16384),
                    8 => (range.start(), range.start() + 8193, 16384),
                    _ => (range.start(), range.start() + 8192, 8192),
                };
                request.query = IndexedResourceQuery::Source {
                    range: index::ByteRange { start, end },
                    bytes,
                };
            }
        }
        assert!(
            fixture
                .reader
                .prepare_indexed_resource(request, &cancel)
                .is_err(),
            "fault {fault}"
        );
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        drop(
            fixture
                .reader
                .prepare_indexed_resource(valid.clone(), &cancel)
                .unwrap(),
        );
    }
    assert!(fixture.reader.retire());
}

#[test]
fn table_source_without_companion_remains_pending_and_releases_custody() {
    let fixture = Fixture::new();
    let (_, metadata) = seed_table(&fixture, "x|y|z\n", 1);
    let ResourceBacking::TextRange { range, .. } = metadata.backing() else {
        panic!("table source")
    };
    let mut request = table_request(&fixture);
    request.query = IndexedResourceQuery::Source {
        range: index::ByteRange {
            start: range.start(),
            end: range.end(),
        },
        bytes: 16384,
    };
    let cancel = AtomicBool::new(false);
    request.root = Some([19; 32]);
    assert!(matches!(
        fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel),
        Err(TranscriptAttachmentError::Unavailable)
    ));
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    request.root = None;
    let prepared = fixture
        .reader
        .prepare_indexed_resource(request.clone(), &cancel)
        .unwrap();
    assert!(matches!(
        prepared.response().value,
        IndexedResourceValue::Pending
    ));
    assert!(prepared.response().root.is_none());
    assert_eq!(prepared.response().encoded_bytes, 1);
    fixture
        .reader
        .publish_indexed_resource_if_current(&prepared, &request, &cancel, |_| ())
        .unwrap();
    drop(prepared);
    assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
    assert!(fixture.reader.retire());
}

#[test]
fn table_source_publication_rejects_replaced_demand_history_backing_and_lifetime() {
    for changed in 0..8 {
        let fixture = Fixture::new();
        let (_, metadata) = seed_table(&fixture, "x|y|z\n", 1);
        complete_with_limits(&fixture, &metadata, 16384, 64);
        let request = source_request(&fixture, &metadata);
        let cancel = AtomicBool::new(false);
        let prepared = fixture
            .reader
            .prepare_indexed_resource(request.clone(), &cancel)
            .unwrap();
        let mut expected = request.clone();
        let error = match changed {
            0 => {
                expected.root = Some([19; 32]);
                TranscriptAttachmentError::Identity
            }
            1 => {
                expected.revision = ProjectionRevision::new(8).unwrap();
                TranscriptAttachmentError::Identity
            }
            2 | 3 => {
                let IndexedResourceQuery::Source { range, bytes } = &mut expected.query else {
                    panic!("source query")
                };
                if changed == 2 {
                    range.end -= 1;
                } else {
                    *bytes -= 1;
                }
                TranscriptAttachmentError::Identity
            }
            4 => {
                fixture.seed_head(2);
                TranscriptAttachmentError::Stale
            }
            5 => {
                seed_table(&fixture, "replacement|y|z\n", 1);
                TranscriptAttachmentError::Stale
            }
            6 => {
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
                &prepared,
                &expected,
                &cancel,
                |_| called.store(true, Ordering::Release)
            ),
            Err(error)
        );
        drop(prepared);
        assert!(!called.load(Ordering::Acquire));
        assert_eq!(fixture.home.retained_frozen_read_count().unwrap(), 0);
        assert!(fixture.reader.retire());
    }
}
