use super::*;
use crate::{
    ContentByteSpanRecord, ContentChunkOrdinal, ContentEncoding, ContentPieceOrdinal,
    ContentReference, ContentSummary, ContentTextSpanRecord, ProjectionSourceRange,
    ProviderFrameOrdinalV1, ProviderNarrativeGeneration, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, ResourceBacking, ResourceKind, ResourceMetadataRecord,
    ResourceOrdinal, ResourceStructure,
};
use beryl_model::*;
use std::{cell::RefCell, collections::BTreeMap};

#[path = "resource_structure_display.rs"]
mod display;

#[derive(Default)]
struct Records {
    values: BTreeMap<(&'static str, Vec<u8>), Vec<u8>>,
    calls: RefCell<Vec<&'static str>>,
}

impl Records {
    fn put<F: Family>(&mut self, key: F::Key, value: F::Value) {
        self.values.insert(
            (F::NAME, F::encode_key(&key).unwrap()),
            F::encode_value(&value).unwrap(),
        );
    }

    fn chunk_at(&mut self, ordinal: u64, start: u64, bytes: &[u8]) {
        let chunk = ContentChunkRecord::new(
            content_id(),
            ContentChunkOrdinal::new(ordinal).unwrap(),
            bytes,
        )
        .unwrap();
        let span = ContentByteSpanRecord::for_chunk(&chunk, start).unwrap();
        self.put::<ContentByteSpansFamily>(
            ContentByteSpanKey {
                owner: content_id(),
                start,
            },
            span,
        );
        self.put::<ContentChunksFamily>(
            ContentChunkKey {
                owner: content_id(),
                ordinal: chunk.ordinal(),
            },
            chunk,
        );
    }

    fn text(
        &mut self,
        logical_start: u64,
        logical_end: u64,
        ordinal: u64,
        chunk_start: u64,
        encoded_start: u64,
    ) {
        let span = ContentTextSpanRecord::new(
            content_id(),
            ContentPieceOrdinal::new(ordinal).unwrap(),
            ContentChunkOrdinal::new(ordinal).unwrap(),
            chunk_start,
            logical_start,
            logical_end,
            encoded_start,
            encoded_start + logical_end - logical_start,
            false,
            [0; 32],
        )
        .unwrap();
        self.put::<ContentTextSpansFamily>(
            ContentTextSpanKey {
                owner: content_id(),
                logical_start,
            },
            span,
        );
    }
}

impl SourceRecords for Records {
    fn predecessor<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
    ) -> Result<(F::Key, F::Value), ConstructionError> {
        self.calls.borrow_mut().push(F::NAME);
        let first = (F::NAME, F::encode_key(&first)?);
        let last = (F::NAME, F::encode_key(&last)?);
        let ((_, key), value) = self
            .values
            .range(first..=last)
            .next_back()
            .ok_or(ConstructionError::Corrupt)?;
        Ok((F::decode_key(key)?, F::decode_value(value)?))
    }

    fn chunk(&self, key: &ContentChunkKey) -> Result<ContentChunkRecord, ConstructionError> {
        self.calls.borrow_mut().push(ContentChunksFamily::NAME);
        let bytes = self
            .values
            .get(&(
                ContentChunksFamily::NAME,
                ContentChunksFamily::encode_key(key)?,
            ))
            .ok_or(ConstructionError::Corrupt)?;
        Ok(ContentChunksFamily::decode_value(bytes)?)
    }
}

fn content_id() -> SyndicContentId {
    SyndicContentId::from_bytes([3; 16])
}

fn composer(length: u64) -> ProjectionTextSource {
    ProjectionTextSource::Composer(ContentReference::new(
        content_id(),
        ContentRevision::new(1).unwrap(),
        ContentEncoding::ComposerV1,
        ContentSummary::new(
            1,
            1,
            length,
            length,
            1,
            0,
            [0; 32],
            None,
            SyndicContentDigest::from_bytes([4; 32]),
        )
        .unwrap(),
    ))
}

fn binding(source: ProjectionTextSource, start: u64, end: u64) -> Binding {
    let id = SyndicResourceId::from_bytes([1; 16]);
    let revision = ProjectionRevision::new(1).unwrap();
    let metadata = ResourceMetadataRecord::new(
        id,
        revision,
        SyndicProjectionId::from_bytes([2; 16]),
        SyndicItemId::from_bytes([5; 16]),
        ResourceOrdinal::FIRST,
        ResourceKind::Code,
        "text/plain",
        ResourceBacking::TextRange {
            source,
            range: ProjectionSourceRange::new(start, end).unwrap(),
        },
        [0; 32],
        None,
        ResourceStructure::code(None, 1).unwrap(),
    )
    .unwrap();
    Binding {
        resource: super::super::ResourceKey { id, revision },
        metadata,
        source,
        range: ByteRange { start, end },
    }
}

#[test]
fn composer_window_preserves_split_utf8_and_relative_resource_offsets() {
    let mut records = Records::default();
    records.chunk_at(1, 0, b"xxA\xe2");
    records.chunk_at(2, 4, b"\x82\xacB\r\n");
    records.text(0, 2, 1, 0, 2);
    records.text(2, 7, 2, 4, 4);
    let binding = binding(composer(7), 1, 6);
    let mut ledger = Ledger::default();
    let bytes = read_authenticated_window(&records, &mut ledger, &binding, 9, 0, 5).unwrap();
    assert_eq!(bytes, "€B\r".as_bytes());
    assert_eq!(ledger.records, 6);
    assert_eq!(
        records
            .calls
            .borrow()
            .iter()
            .filter(|family| **family == ContentChunksFamily::NAME)
            .count(),
        2
    );
    assert!(ledger.retained >= bytes.len());
}

#[test]
fn provider_window_crosses_physical_chunks_and_uses_retained_frontier() {
    let mut records = Records::default();
    records.chunk_at(1, 0, b"xxabc");
    records.chunk_at(2, 5, b"defZZ");
    let generation = ProviderNarrativeGeneration::new(1).unwrap();
    let span = ProviderNarrativeSpanRecord::new(
        content_id(),
        generation,
        0,
        6,
        ProviderFrameOrdinalV1::new(1).unwrap(),
        [1; 32],
        2,
        8,
        [2; 32],
        [3; 32],
    )
    .unwrap();
    let narrative = ProviderNarrativeReference::new(
        content_id(),
        generation,
        1,
        6,
        span.resulting_chain_digest(),
    )
    .unwrap();
    records.put::<ProviderNarrativeSpansFamily>(
        ProviderNarrativeSpanKey::new(content_id(), generation, 0),
        span,
    );
    let binding = binding(ProjectionTextSource::ProviderNarrative(narrative), 0, 6);
    let bytes =
        read_authenticated_window(&records, &mut Ledger::default(), &binding, 10, 1, 5).unwrap();
    assert_eq!(bytes, b"bcdef");
    let wrong = ProviderNarrativeReference::new(content_id(), generation, 1, 6, [0; 32]).unwrap();
    assert!(matches!(
        read_authenticated_window(
            &records,
            &mut Ledger::default(),
            &self::binding(ProjectionTextSource::ProviderNarrative(wrong), 0, 6),
            10,
            0,
            1
        ),
        Err(ConstructionError::Corrupt)
    ));
}

#[test]
fn late_window_seeks_only_demanded_span_and_caches_shared_chunk() {
    let mut records = Records::default();
    records.chunk_at(1, 0, b"prefixABC");
    records.text(90, 91, 1, 0, 6);
    records.text(91, 93, 1, 0, 7);
    let mut ledger = Ledger::default();
    assert_eq!(
        read_authenticated_window(
            &records,
            &mut ledger,
            &binding(composer(93), 90, 93),
            9,
            0,
            3
        )
        .unwrap(),
        b"ABC"
    );
    assert_eq!(ledger.records, 5);
    assert_eq!(
        records
            .calls
            .borrow()
            .iter()
            .filter(|family| **family == ContentChunksFamily::NAME)
            .count(),
        1
    );
}

#[test]
fn ledger_refuses_before_span_chunk_or_output_acquisition() {
    let mut records = Records::default();
    records.chunk_at(1, 0, b"A");
    records.text(0, 1, 1, 0, 0);
    let binding = binding(composer(1), 0, 1);
    for initial in [
        Ledger {
            records: 520,
            ..Ledger::default()
        },
        Ledger {
            retained: 524_288,
            ..Ledger::default()
        },
    ] {
        records.calls.borrow_mut().clear();
        let mut ledger = initial;
        assert!(matches!(
            read_authenticated_window(&records, &mut ledger, &binding, 1, 0, 1),
            Err(ConstructionError::Limit)
        ));
        assert!(records.calls.borrow().is_empty());
    }
    let mut ledger = Ledger {
        read_bytes: 524_288 - 1_024,
        ..Ledger::default()
    };
    assert!(matches!(
        read_authenticated_window(&records, &mut ledger, &binding, 1, 0, 1),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(
        *records.calls.borrow(),
        vec![ContentTextSpansFamily::NAME, ContentByteSpansFamily::NAME]
    );
}

#[test]
fn separate_visible_fragments_share_one_charged_chunk_and_release_with_the_ledger() {
    let mut records = Records::default();
    let text = b"one\n".repeat(16);
    records.chunk_at(1, 0, &text);
    records.text(0, 64, 1, 0, 0);
    let binding = binding(composer(64), 0, 64);
    let mut ledger = Ledger::default();
    for line in 0..16 {
        assert_eq!(
            read_authenticated_window(&records, &mut ledger, &binding, 64, line * 4, 3).unwrap(),
            b"one"
        );
    }
    assert_eq!(
        records
            .calls
            .borrow()
            .iter()
            .filter(|family| **family == ContentChunksFamily::NAME)
            .count(),
        1
    );
    let retained = std::sync::Arc::downgrade(ledger.source_chunk.as_ref().unwrap());
    drop(ledger);
    assert!(retained.upgrade().is_none());
    read_authenticated_window(&records, &mut Ledger::default(), &binding, 64, 0, 3).unwrap();
    assert_eq!(
        records
            .calls
            .borrow()
            .iter()
            .filter(|family| **family == ContentChunksFamily::NAME)
            .count(),
        2
    );
}

#[test]
fn source_window_rejects_gaps_chunk_substitution_and_bad_mapping() {
    for fault in 0..4 {
        let mut records = Records::default();
        records.chunk_at(1, 0, b"abc");
        records.text(
            if fault == 0 { 1 } else { 0 },
            3,
            1,
            if fault == 1 { 1 } else { 0 },
            if fault == 1 { 1 } else { 0 },
        );
        if fault == 2 {
            let chunk =
                ContentChunkRecord::new(content_id(), ContentChunkOrdinal::FIRST, &b"xyz"[..])
                    .unwrap();
            records.put::<ContentChunksFamily>(
                ContentChunkKey {
                    owner: content_id(),
                    ordinal: ContentChunkOrdinal::FIRST,
                },
                chunk,
            );
        }
        assert!(matches!(
            read_authenticated_window(
                &records,
                &mut Ledger::default(),
                &binding(composer(3), 0, 3),
                if fault == 3 { 2 } else { 3 },
                0,
                3
            ),
            Err(ConstructionError::Corrupt)
        ));
    }
}

#[test]
fn fragmented_source_refuses_full_window_and_accepts_smaller_exact_window() {
    let mut records = Records::default();
    records.chunk_at(1, 0, &vec![b'x'; 300]);
    for position in 0..300 {
        records.text(position, position + 1, 1, 0, position);
    }
    let binding = binding(composer(300), 0, 300);
    let mut ledger = Ledger::default();
    assert!(matches!(
        read_authenticated_window(&records, &mut ledger, &binding, 300, 0, 300),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(ledger.records, 520);
    assert_eq!(records.calls.borrow().len(), 520);
    records.calls.borrow_mut().clear();
    let bytes =
        read_authenticated_window(&records, &mut Ledger::default(), &binding, 300, 0, 200).unwrap();
    assert_eq!(bytes, vec![b'x'; 200]);
    assert_eq!(records.calls.borrow().len(), 401);
}

#[test]
fn empty_eof_and_invalid_requests_acquire_no_source_records() {
    let records = Records::default();
    let binding = binding(composer(8), 2, 8);
    assert!(
        read_authenticated_window(&records, &mut Ledger::default(), &binding, 8, 6, 0)
            .unwrap()
            .is_empty()
    );
    for (start, length) in [(7, 0), (5, 2), (u64::MAX, 1), (0, 65_537)] {
        assert!(
            read_authenticated_window(&records, &mut Ledger::default(), &binding, 8, start, length)
                .is_err()
        );
    }
    assert!(records.calls.borrow().is_empty());
}
