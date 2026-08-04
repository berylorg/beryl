use super::*;

#[test]
fn more_than_1024_markers_stage_in_fixed_pages_and_public_reads_clamp_exactly() {
    const MARKER_COUNT: u64 = 1_088;

    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, state);
    let set_id = AssetReferenceSetId::from_bytes([41; 16]);
    let markers = (1..=MARKER_COUNT)
        .map(|index| (marker(index), ImageLabelOrdinal::FIRST))
        .collect::<Vec<_>>();
    let source = marker_summary(markers.iter().copied());
    let staging = begin_reference_set(&store, state, set_id, source);

    let mut append_commands = 0;
    for marker_page in markers.chunks(ASSET_REFERENCE_PAGE_MAX_ENTRIES) {
        assert_eq!(marker_page.len(), ASSET_REFERENCE_PAGE_MAX_ENTRIES);
        let manifest = state
            .assets()
            .staged_reference_set_manifest(&store, staging)
            .unwrap();
        let entries = marker_page
            .iter()
            .map(|(marker_id, label)| AssetReferencePageEntry::new(*marker_id, *label, asset_id))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let append = AppendAssetReferencePage::new(manifest.build_proof(), entries).unwrap();
        assert_eq!(append.entries().len(), ASSET_REFERENCE_PAGE_MAX_ENTRIES);
        execute(
            &store,
            state
                .assets()
                .append_reference_page(state.assets().revision(&store).unwrap(), append),
        );
        append_commands += 1;
    }
    assert_eq!(append_commands, 17);

    let manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    assert_eq!(manifest.marker_count(), MARKER_COUNT);
    assert_eq!(manifest.entry_frontier(), MARKER_COUNT);
    let build = manifest.build_proof();
    let sealed = build.sealed_proof().unwrap();
    execute(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    let sealed_manifest = state
        .assets()
        .sealed_reference_set_manifest(&store, sealed)
        .unwrap();
    assert_eq!(sealed_manifest.sealed_proof(), Some(sealed));

    let package_maximum = state
        .assets()
        .reference_set_entries(
            &store,
            sealed,
            None,
            CursorReadLimits::new(
                ASSET_REFERENCE_PAGE_MAX_ENTRIES,
                ASSET_REFERENCE_PAGE_MAX_STORED_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
    let oversized_request = state
        .assets()
        .reference_set_entries(
            &store,
            sealed,
            None,
            CursorReadLimits::new(10_000, 10_000_000).unwrap(),
        )
        .unwrap();
    assert_eq!(oversized_request, package_maximum);
    assert_eq!(package_maximum.records().len(), 64);
    assert!(package_maximum.has_more());
    assert!(package_maximum.stored_bytes() <= ASSET_REFERENCE_PAGE_MAX_STORED_BYTES);
    assert!(package_maximum.decoded_bytes() > 0);

    let exact_bytes = package_maximum.stored_bytes();
    let exact_page = state
        .assets()
        .reference_set_entries(
            &store,
            sealed,
            None,
            CursorReadLimits::new(ASSET_REFERENCE_PAGE_MAX_ENTRIES, exact_bytes).unwrap(),
        )
        .unwrap();
    assert_eq!(exact_page, package_maximum);
    let one_byte_short = state
        .assets()
        .reference_set_entries(
            &store,
            sealed,
            None,
            CursorReadLimits::new(ASSET_REFERENCE_PAGE_MAX_ENTRIES, exact_bytes - 1).unwrap(),
        )
        .unwrap();
    assert!(one_byte_short.records().len() < ASSET_REFERENCE_PAGE_MAX_ENTRIES);
    assert!(one_byte_short.has_more());
    assert!(one_byte_short.stored_bytes() < exact_bytes);
}

#[test]
fn append_rejects_stale_cross_page_duplicate_mismatched_label_and_missing_metadata() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (first_asset, _) = publish_metadata_bytes(
        &store,
        state,
        b"first-append-asset",
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let (second_asset, _) = publish_metadata_bytes(
        &store,
        state,
        b"second-append-asset",
        AssetMediaType::new("image/webp").unwrap(),
        None,
    );
    let missing_asset = AssetId::sha256_v1([99; 32], NonZeroU64::new(99).unwrap());
    let set_id = AssetReferenceSetId::from_bytes([42; 16]);
    let second_marker = marker(2);
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let source = marker_summary([
        (marker(1), ImageLabelOrdinal::FIRST),
        (second_marker, second_label),
    ]);
    let staging = begin_reference_set(&store, state, set_id, source);
    let stale_proof = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap()
        .build_proof();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                stale_proof,
                Box::from([AssetReferencePageEntry::new(
                    marker(1),
                    ImageLabelOrdinal::FIRST,
                    first_asset,
                )]),
            )
            .unwrap(),
        ),
    );
    let current = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();

    let stale = execute_result(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                stale_proof,
                Box::from([AssetReferencePageEntry::new(
                    second_marker,
                    second_label,
                    first_asset,
                )]),
            )
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        asset_mutation_error(&stale),
        AssetMutationError::BuildProofMismatch(actual) if *actual == set_id
    ));

    let duplicate = execute_result(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                current.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    marker(1),
                    second_label,
                    first_asset,
                )]),
            )
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        asset_mutation_error(&duplicate),
        AssetMutationError::MarkerAlreadyExists(actual) if *actual == marker(1)
    ));

    let mismatched_label = execute_result(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                current.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    second_marker,
                    ImageLabelOrdinal::FIRST,
                    second_asset,
                )]),
            )
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        asset_mutation_error(&mismatched_label),
        AssetMutationError::LabelAssetMismatch { label }
            if *label == ImageLabelOrdinal::FIRST
    ));

    let missing_metadata = execute_result(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                current.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    second_marker,
                    second_label,
                    missing_asset,
                )]),
            )
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        asset_mutation_error(&missing_metadata),
        AssetMutationError::MetadataMissing(actual) if *actual == missing_asset
    ));

    assert_eq!(
        state
            .assets()
            .staged_reference_set_manifest(&store, staging)
            .unwrap(),
        current
    );
}
