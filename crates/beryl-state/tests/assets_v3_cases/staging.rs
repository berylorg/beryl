use super::*;

#[test]
fn more_than_1024_markers_stage_v3_in_fixed_pages_and_public_reads_clamp_exactly() {
    const MARKER_COUNT: u64 = 1_088;

    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([41; 16]);
    let markers = (1..=MARKER_COUNT)
        .map(|index| (marker(index), ImageLabelOrdinal::FIRST))
        .collect::<Vec<_>>();
    let source = marker_summary(markers.iter().copied());
    let staging = begin_reference_set(&store, &state, set_id);

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
    let seal = SealAssetReferenceSet::new(build, source, build.ordered_assets()).unwrap();
    let sealed = seal.sealed_proof();
    execute(
        &store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(&store).unwrap(), seal),
    );
    let sealed_manifest = state
        .assets()
        .sealed_reference_set_manifest(&store, sealed)
        .unwrap();
    assert_eq!(sealed_manifest.sequential(), sealed.sequential());
    assert_eq!(sealed_manifest.ordered_assets(), sealed.ordered_assets());

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
        &state,
        b"first-append-asset",
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let (second_asset, _) = publish_metadata_bytes(
        &store,
        &state,
        b"second-append-asset",
        AssetMediaType::new("image/webp").unwrap(),
        None,
    );
    let missing_asset = AssetId::sha256_v1([99; 32], NonZeroU64::new(99).unwrap());
    let set_id = AssetReferenceSetId::from_bytes([42; 16]);
    let second_marker = marker(2);
    let second_label = ImageLabelOrdinal::new(2).unwrap();
    let staging = begin_reference_set(&store, &state, set_id);
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

    let stale = execute_outcome(
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
    );
    let CommandOutcome::NotCommitted { evidence: stale } = stale else {
        panic!("expected rejected stale asset command, got {stale:?}");
    };
    assert!(matches!(
        asset_mutation_error(&stale),
        AssetMutationError::BuildProofMismatch(actual) if *actual == set_id
    ));

    let duplicate = execute_outcome(
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
    );
    let CommandOutcome::NotCommitted {
        evidence: duplicate,
    } = duplicate
    else {
        panic!("expected rejected duplicate asset command, got {duplicate:?}");
    };
    assert!(matches!(
        asset_mutation_error(&duplicate),
        AssetMutationError::MarkerAlreadyExists(actual) if *actual == marker(1)
    ));

    let mismatched_label = execute_outcome(
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
    );
    let CommandOutcome::NotCommitted {
        evidence: mismatched_label,
    } = mismatched_label
    else {
        panic!("expected rejected label-mismatch asset command, got {mismatched_label:?}");
    };
    assert!(matches!(
        asset_mutation_error(&mismatched_label),
        AssetMutationError::LabelAssetMismatch { label }
            if *label == ImageLabelOrdinal::FIRST
    ));

    let missing_metadata = execute_outcome(
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
    );
    let CommandOutcome::NotCommitted {
        evidence: missing_metadata,
    } = missing_metadata
    else {
        panic!("expected rejected missing-metadata asset command, got {missing_metadata:?}");
    };
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

#[test]
fn seal_rejects_forged_sequential_and_swapped_asset_summaries_without_publication() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (first_asset, _) = publish_metadata_bytes(
        &store,
        &state,
        b"first-seal-summary-asset",
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let (second_asset, _) = publish_metadata_bytes(
        &store,
        &state,
        b"second-seal-summary-asset",
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let set_id = AssetReferenceSetId::from_bytes([66; 16]);
    let first = (marker(1), ImageLabelOrdinal::FIRST, first_asset);
    let second = (marker(2), ImageLabelOrdinal::new(2).unwrap(), second_asset);
    let sequential = marker_summary([(first.0, first.1), (second.0, second.1)]);
    let staging = begin_reference_set(&store, &state, set_id);
    let initial = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                initial.build_proof(),
                Box::from([
                    AssetReferencePageEntry::new(first.0, first.1, first.2),
                    AssetReferencePageEntry::new(second.0, second.1, second.2),
                ]),
            )
            .unwrap(),
        ),
    );
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let swapped =
        ordered_marker_asset_summary([(first.0, first.1, second.2), (second.0, second.1, first.2)]);
    assert_ne!(swapped, building.ordered_assets());
    let rejected = execute_outcome(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(building.build_proof(), sequential, swapped).unwrap(),
        ),
    );
    let CommandOutcome::NotCommitted { evidence: rejected } = rejected else {
        panic!("expected rejected forged ordered summary, got {rejected:?}");
    };
    assert!(matches!(
        asset_mutation_error(&rejected),
        AssetMutationError::MarkerSummaryMismatch(actual) if *actual == set_id
    ));

    let forged_sequential = SequentialMarkerSummaryV1::new(
        [u8::MAX; 32],
        sequential.marker_count(),
        sequential.maximum_image_label(),
    )
    .unwrap();
    let rejected = execute_outcome(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(
                building.build_proof(),
                forged_sequential,
                building.ordered_assets(),
            )
            .unwrap(),
        ),
    );
    let CommandOutcome::NotCommitted { evidence: rejected } = rejected else {
        panic!("expected rejected forged sequential summary, got {rejected:?}");
    };
    assert!(matches!(
        asset_mutation_error(&rejected),
        AssetMutationError::MarkerSummaryMismatch(actual) if *actual == set_id
    ));
    assert_eq!(
        state
            .assets()
            .staged_reference_set_manifest(&store, staging)
            .unwrap(),
        building
    );
}
