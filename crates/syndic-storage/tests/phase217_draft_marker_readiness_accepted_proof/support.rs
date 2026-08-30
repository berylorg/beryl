use super::*;

pub(super) struct AcceptedFixture {
    pub(super) _home: TestHome,
    pub(super) store: HomeStore,
    pub(super) storage: SyndicStorage,
    pub(super) state: BerylState,
    pub(super) thread: SyndicThreadId,
    pub(super) child: SyndicThreadId,
    pub(super) session: DraftEditorCandidateSessionV1,
    pub(super) proof: SealedAssetReferenceSetProof,
    pub(super) label: ImageLabelOrdinal,
    pub(super) asset_id: AssetId,
}

impl AcceptedFixture {
    pub(super) fn new(name: &str, seed: u8) -> Self {
        let (home, mut store, storage, thread) = fixture(name, seed);
        let state = BerylState::register(&mut store).unwrap();
        let label = ImageLabelOrdinal::new(7).unwrap();
        let marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(20); 16]);
        let asset_id = publish_metadata(&store, &state, &[seed; 13]);
        let proof = seal_one_entry_set(
            &store,
            &state,
            AssetReferenceSetId::from_bytes([seed.wrapping_add(21); 16]),
            marker,
            label,
            asset_id,
        );
        publish_local_origin(
            &store,
            storage.clone(),
            thread,
            proof,
            label,
            asset_id,
            marker,
            seed,
        );
        let current = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &current,
            seed.wrapping_add(22),
            seed.wrapping_add(23),
        );
        let child = publish_inheriting_child(&store, storage.clone(), thread, label, seed);
        Self {
            _home: home,
            store,
            storage,
            state,
            thread,
            child,
            session,
            proof,
            label,
            asset_id,
        }
    }

    pub(super) fn association(
        &self,
        target: u8,
        source_thread: SyndicThreadId,
    ) -> DraftMarkerReadinessSourceAssociationV1 {
        DraftMarkerReadinessSourceAssociationV1::new(
            SyndicDraftMarkerId::from_bytes([target; 16]),
            DraftMarkerReadinessSourceSelectorV1::Accepted(
                DraftMarkerReadinessAcceptedSourceV1::new(
                    source_thread,
                    self.proof,
                    self.label,
                    self.asset_id,
                ),
            ),
        )
    }

    pub(super) fn factory(&self) -> DraftMarkerReadinessWitnessFactoryV1 {
        DraftMarkerReadinessWitnessFactoryV1::new(
            self.state
                .assets()
                .draft_marker_label_readiness_witness_factory(),
        )
    }
}

pub(super) fn execute_asset(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub(super) fn advance_asset_revision(fixture: &AcceptedFixture, seed: u8) {
    let staging = AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([seed; 16]),
        [seed; 32],
    );
    execute_asset(
        &fixture.store,
        fixture.state.assets().begin_reference_set(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            BeginAssetReferenceSet::new(staging),
        ),
    );
}

pub(super) fn advance_syndic_revision(fixture: &AcceptedFixture) {
    let mut records = FixtureBatch::new();
    records
        .put(FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                fixture.thread,
                3,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::from_raw(fixture.label.get()),
            )
            .unwrap(),
        ))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), records);
}

fn publish_metadata(store: &HomeStore, state: &BerylState, bytes: &[u8]) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            beryl_home_store::SidecarNamespace::new("images").unwrap(),
            bytes,
            beryl_home_store::SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset_id = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset_id,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    asset_id
}

fn seal_one_entry_set(
    store: &HomeStore,
    state: &BerylState,
    set_id: AssetReferenceSetId,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> SealedAssetReferenceSetProof {
    let staging = AssetReferenceSetStagingAuthority::new(set_id, [set_id.as_bytes()[0]; 32]);
    execute_asset(
        store,
        state.assets().begin_reference_set(
            state.assets().revision(store).unwrap(),
            BeginAssetReferenceSet::new(staging),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    execute_asset(
        store,
        state.assets().append_reference_page(
            state.assets().revision(store).unwrap(),
            AppendAssetReferencePage::new(
                manifest.build_proof(),
                Box::from([AssetReferencePageEntry::new(marker_id, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let manifest = state
        .assets()
        .staged_reference_set_manifest(store, staging)
        .unwrap();
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        marker_summary([(marker_id, label)]),
        ordered_summary([(marker_id, label, asset_id)]),
    )
    .unwrap();
    let proof = seal.sealed_proof();
    execute_asset(
        store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(store).unwrap(), seal),
    );
    proof
}

fn marker_summary(
    markers: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal)>,
) -> SequentialMarkerSummaryV1 {
    let mut digest = sequential_marker_digest_seed();
    let mut count = 0;
    let mut maximum = None;
    for (marker, label) in markers {
        digest = advance_sequential_marker_digest(digest, marker, label);
        count += 1;
        maximum = Some(maximum.map_or(label, |prior: ImageLabelOrdinal| prior.max(label)));
    }
    SequentialMarkerSummaryV1::new(digest, count, maximum).unwrap()
}

fn ordered_summary(
    entries: impl IntoIterator<Item = (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId)>,
) -> OrderedMarkerAssetSummaryV1 {
    let mut digest = ordered_marker_asset_digest_seed();
    let mut count = 0;
    for (marker, label, asset_id) in entries {
        digest = advance_ordered_marker_asset_digest(digest, marker, label, asset_id);
        count += 1;
    }
    OrderedMarkerAssetSummaryV1::new(digest, count)
}

fn publish_local_origin(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    proof: SealedAssetReferenceSetProof,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
    marker: SyndicDraftMarkerId,
    seed: u8,
) {
    let source_draft = SyndicDraftId::from_bytes([seed.wrapping_add(30); 16]);
    let replacement_draft = SyndicDraftId::from_bytes([seed.wrapping_add(31); 16]);
    let input_id = source_draft.accepted_input_id();
    let payload = ComposerPayload::new(vec![ComposerAtom::image_marker(marker, label)]).unwrap();
    let content = PreparedContent::composer(&payload)
        .unwrap()
        .reference(ContentRevision::new(1).unwrap());
    let input = AcceptedInputRecord::new(
        input_id,
        thread,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputAdmissionProof::new(
            ThreadRevision::new(1).unwrap(),
            source_draft,
            DraftRevision::new(1).unwrap(),
            InputGateRevision::new(1).unwrap(),
            replacement_draft,
        )
        .unwrap(),
        AcceptedRouteGeneration::FIRST,
        content,
        Some(proof),
        crate::support::timestamp(u64::from(seed) + 100),
    )
    .unwrap();
    let span = ImageLabelOriginSpanRecord::new(
        thread,
        ImageLabelOrdinal::FIRST,
        label,
        ImageLabelOriginOwner::AcceptedInput(input_id),
        proof,
    )
    .unwrap();
    let mut records = FixtureBatch::new();
    records.put(FixtureRecord::AcceptedInput(input)).unwrap();
    records
        .put(FixtureRecord::ImageLabelOriginSpan(span))
        .unwrap();
    records
        .put(FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                thread,
                2,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::from_raw(label.get()),
            )
            .unwrap(),
        ))
        .unwrap();
    records
        .put(FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(
                thread,
                2,
                ImageLabelFrontier::from_raw(label.get()),
            )
            .unwrap(),
        ))
        .unwrap();
    crate::support::commit(store, storage, records);
    let _ = asset_id;
}

fn publish_inheriting_child(
    store: &HomeStore,
    storage: SyndicStorage,
    parent: SyndicThreadId,
    label: ImageLabelOrdinal,
    seed: u8,
) -> SyndicThreadId {
    let parent_record = storage
        .thread(store, parent, SyndicPointReadLimit::new(1 << 20).unwrap())
        .unwrap()
        .unwrap();
    let child = SyndicThreadId::from_bytes([seed.wrapping_add(40); 16]);
    let digest = child_thread_lineage_digest(child, parent, parent_record.lineage_digest());
    let record = ThreadRecord::new(
        child,
        SelectedPathProof::new(
            None,
            ThreadRevision::new(1).unwrap(),
            empty_selected_path_digest(),
        ),
        SyndicDraftId::from_bytes([seed.wrapping_add(41); 16]),
        ThreadLineageProof::new(
            Some(parent),
            Some(parent),
            ThreadLineageDepth::new(2).unwrap(),
            digest,
        ),
        None,
    );
    let mut records = FixtureBatch::new();
    records.put(FixtureRecord::Thread(record)).unwrap();
    records
        .put(FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                child,
                1,
                ImageLabelFrontier::from_raw(label.get()),
                ImageLabelFrontier::from_raw(label.get()),
            )
            .unwrap(),
        ))
        .unwrap();
    crate::support::commit(store, storage, records);
    child
}

pub(super) fn manual_accepted_entry(
    proof: SealedAssetReferenceSetProof,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(194);
    let sequential = proof.sequential();
    let ordered = proof.ordered_assets();
    bytes.push(1);
    bytes.extend_from_slice(proof.set_id().as_bytes());
    bytes.extend_from_slice(&sequential.marker_digest());
    bytes.extend_from_slice(&sequential.marker_count().to_le_bytes());
    bytes.extend_from_slice(
        &sequential
            .maximum_image_label()
            .map_or(0, ImageLabelOrdinal::get)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&ordered.marker_asset_digest());
    bytes.extend_from_slice(&ordered.marker_count().to_le_bytes());
    bytes.extend_from_slice(&proof.entry_frontier().to_le_bytes());
    bytes.extend_from_slice(&proof.asset_chain_digest().as_bytes());
    bytes.extend_from_slice(&label.get().to_le_bytes());
    bytes.push(asset_id.version() as u8);
    bytes.extend_from_slice(&asset_id.digest());
    bytes.extend_from_slice(&asset_id.length().get().to_le_bytes());
    bytes
}

pub(super) fn manual_correlation(ordinal: NonZeroU64, eof: bool, entries: &[Vec<u8>]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"syndic/draft-marker-label-readiness-page/v1");
    hasher.update(ordinal.get().to_le_bytes());
    hasher.update([u8::from(eof)]);
    hasher.update((entries.len() as u64).to_le_bytes());
    for entry in entries {
        hasher.update(entry);
    }
    hasher.finalize().into()
}
