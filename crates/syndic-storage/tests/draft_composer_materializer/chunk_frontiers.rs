use super::*;

#[test]
fn split_text_headers_and_utf8_payload_preserve_canonical_chunks_on_reopen() {
    for split in 0..=13 {
        assert_boundary_materialization(split, false);
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn split_marker_atoms_preserve_complete_piece_coordinates_on_reopen() {
    for split in 0..=25 {
        assert_boundary_materialization(split, true);
    }
}

fn assert_boundary_materialization(split: usize, with_marker: bool) {
    let (home, mut store, mut storage, thread) = fixture("chunk-frontier", 181);
    let first = "a".repeat(32_000);
    let second = "b".repeat(32_485 - split);
    replace_empty(
        &storage,
        &store,
        thread,
        182,
        vec![DraftPieceV1::Text(first.clone())],
    );
    append_text(&storage, &store, thread, 183, &second);
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([184; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        AssetId::sha256_v1([184; 32], std::num::NonZeroU64::MIN),
    );
    let tail = "💎z";
    let end = (first.len() + second.len()) as u64;
    if with_marker {
        apply_replacement_with_caret(
            &storage,
            &store,
            thread,
            184,
            DraftPieceReplacementV1::new(
                point(end),
                point(end),
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    end,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
            point(0),
        );
    }
    let after = if with_marker {
        DraftCompositePositionV1::new(end, DraftCompositeGapWitnessV1::AfterAll)
    } else {
        point(end)
    };
    let root = apply_replacement(
        &storage,
        &store,
        thread,
        186,
        DraftPieceReplacementV1::new(after, after, vec![DraftPieceV1::Text(tail.to_owned())]),
    );
    let key = materialization_key(root, 185);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
    ));
    let mut steps = 0;
    let mapping = loop {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(&store, key)
            .unwrap()
        {
            break mapping;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(&store, key)
            .unwrap()
            .unwrap();
        assert!(prepared.records_read() <= DRAFT_COMPOSER_READ_MAX_RECORDS);
        assert!(prepared.input_payload_bytes() <= DRAFT_COMPOSER_INPUT_MAX_BYTES);
        assert!(prepared.written_record_count() <= DRAFT_COMPOSER_WRITE_MAX_RECORDS);
        assert!(prepared.resident_bytes() <= DRAFT_COMPOSER_RESIDENT_MAX_BYTES);
        committed(execute(
            &store,
            storage.advance_draft_composer_materialization(
                storage.revision(&store).unwrap(),
                prepared,
            ),
        ));
        drop(store);
        let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        storage = SyndicStorage::register(&mut candidate).unwrap();
        store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        steps += 1;
        assert!(steps < 64, "split {split}, marker {with_marker}");
    };
    let mut expected = vec![1];
    expected.extend_from_slice(&(if with_marker { 4_u64 } else { 3 }).to_be_bytes());
    append_text_atom(&mut expected, &first);
    append_text_atom(&mut expected, &second);
    let marker_start = expected.len() as u64;
    if with_marker {
        append_marker_atom(&mut expected, marker);
    }
    append_text_atom(&mut expected, tail);
    let chunks = storage
        .content_chunks(
            &store,
            mapping.content().id(),
            None,
            CursorReadLimits::new(4, 131_072).unwrap(),
        )
        .unwrap();
    let actual: Vec<u8> = chunks
        .records()
        .iter()
        .flat_map(|chunk| chunk.bytes().iter().copied())
        .collect();
    assert_eq!(actual, expected, "split {split}, marker {with_marker}");
    let expected_first_chunk = if !with_marker && (10..=12).contains(&split) {
        64_512 - (split - 9)
    } else {
        64_512
    };
    assert_eq!(chunks.records()[0].bytes().len(), expected_first_chunk);
    assert_eq!(
        mapping.content().summary().logical_utf8_bytes(),
        (first.len() + second.len() + tail.len()) as u64
    );
    assert_eq!(
        mapping.content().summary().image_marker_count(),
        u64::from(with_marker)
    );
    let spans = storage
        .content_text_spans(
            &store,
            mapping.content().id(),
            None,
            CursorReadLimits::new(16, 65_536).unwrap(),
        )
        .unwrap();
    let mut payload = Vec::new();
    for span in spans.records() {
        let chunk = &chunks.records()[span.chunk_ordinal().get() as usize - 1];
        let chunk_start = chunks
            .records()
            .iter()
            .take(span.chunk_ordinal().get() as usize - 1)
            .map(|chunk| chunk.bytes().len() as u64)
            .sum::<u64>();
        assert_eq!(span.chunk_start(), chunk_start);
        assert!(span.encoded_start() >= chunk_start);
        assert!(span.encoded_end() <= chunk_start + chunk.bytes().len() as u64);
        let bytes = &actual[span.encoded_start() as usize..span.encoded_end() as usize];
        assert!(!bytes.is_empty());
        assert!(std::str::from_utf8(bytes).is_ok());
        payload.extend_from_slice(bytes);
    }
    assert_eq!(payload, format!("{first}{second}{tail}").into_bytes());
    if with_marker {
        let pieces = storage
            .content_pieces(
                &store,
                mapping.content().id(),
                None,
                CursorReadLimits::new(16, 65_536).unwrap(),
            )
            .unwrap();
        let markers: Vec<_> = pieces
            .records()
            .iter()
            .filter(|piece| matches!(piece, syndic_storage::ContentPieceRecord::ImageMarker { encoded_start, .. } if *encoded_start == marker_start))
            .collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].encoded_end(), marker_start + 25);
    }
}
