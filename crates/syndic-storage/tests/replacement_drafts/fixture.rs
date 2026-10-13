use super::*;
use std::io::Write;
use std::sync::Mutex;

static LEDGER: Mutex<()> = Mutex::new(());
pub(super) struct Fixture {
    pub home: support::TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub state: BerylState,
    pub faults: FaultController,
    pub thread: beryl_model::SyndicThreadId,
    pub item: SyndicItemId,
    pub session: DraftEditorCandidateSessionV1,
    pub content: ContentReference,
    pub intent: ReplacementEditIntent,
    pub proof: Option<SealedAssetReferenceSetProof>,
    pub asset: Option<AssetId>,
    pub successor_position: DraftCompositePositionV1,
}
pub(super) fn tracked_home(name: &str) -> support::TestHome {
    let home = support::TestHome::new(name);
    if let Some(ledger) = std::env::var_os("BERYL_REPLACEMENT_FIXTURE_LEDGER") {
        let _lock = LEDGER.lock().unwrap();
        let ledger = std::path::PathBuf::from(ledger);
        assert!(ledger.is_absolute());
        let path = home.path().canonicalize().unwrap();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger)
            .unwrap();
        writeln!(file, "{}", path.display()).unwrap();
    }
    home
}
impl Fixture {
    pub fn new(
        name: &str,
        payload: ComposerPayload,
        markers: Vec<(SyndicDraftMarkerId, ImageLabelOrdinal)>,
        budget: u64,
    ) -> Self {
        let successor_position = if payload
            .atoms()
            .iter()
            .find(|atom| !matches!(atom,ComposerAtom::Text(text) if text.is_empty()))
            .is_some_and(|atom| matches!(atom, ComposerAtom::ImageMarker(_)))
        {
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll)
        } else {
            point(0)
        };
        let home = tracked_home(name);
        let faults = FaultController::new();
        let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let thread = ordinary::create_thread(&storage, &store, 201, budget);
        let item = SyndicItemId::from_bytes([204; 16]);
        let (asset, proof) = if markers.is_empty() {
            (None, None)
        } else {
            let asset = super::assets::publish_asset(&store, &state, b"replacement-image");
            let proof = super::assets::seal_set(&store, &state, &markers, asset);
            committed(execute(
                &store,
                state.assets().update_owner_heads(
                    state.assets().revision(&store).unwrap(),
                    UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                        AssetOwner::SubmittedTurnItem(item),
                        None,
                        Some(proof),
                    )]))
                    .unwrap(),
                ),
            ));
            (Some(asset), Some(proof))
        };
        let (content, intent) = seed_source(&store, &storage, thread, item, &payload, proof);
        let current = ordinary::current(&storage, &store, thread);
        let session = ordinary::open_session(&storage, &store, &current, 205, 206);
        Self {
            home,
            store,
            storage,
            state,
            faults,
            thread,
            item,
            session,
            content,
            intent,
            proof,
            asset,
            successor_position,
        }
    }
    pub fn request(&self) -> DraftReplacementImportRequestV1 {
        DraftReplacementImportRequestV1::new(
            ordinary::selector(&ordinary::current(&self.storage, &self.store, self.thread)),
            DraftEditorCandidateActivationBindingV1::from_head(&self.session),
            operation(207),
            self.intent,
            self.item,
            beryl_model::ProjectionRevision::new(1).unwrap(),
            self.content,
            self.storage
                .input_gate(
                    &self.store,
                    self.thread,
                    SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap()
                .revision(),
            self.proof,
            point(0),
            point(0),
            self.successor_position,
            self.successor_position,
        )
    }
    pub fn prepare(&self) -> PreparedDraftReplacementImportV1 {
        self.storage
            .prepare_replacement_draft_import(&self.store, self.request())
            .unwrap()
    }
    pub fn head(&self) -> DraftEditorCandidateSessionV1 {
        match self
            .storage
            .draft_editor_candidate_session(
                &self.store,
                self.session.draft_id(),
                self.session.session_id(),
            )
            .unwrap()
        {
            DraftEditorCandidateSessionReadOutcomeV1::Active(h) => h,
            other => panic!("invalid candidate session: {other:?}"),
        }
    }
    pub fn reopen(self) -> Self {
        let Self {
            home,
            store,
            storage,
            state,
            faults,
            thread,
            item,
            session,
            content,
            intent,
            proof,
            asset,
            successor_position,
        } = self;
        drop(store);
        drop(storage);
        drop(state);
        let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        Self {
            home,
            store,
            storage,
            state,
            faults,
            thread,
            item,
            session,
            content,
            intent,
            proof,
            asset,
            successor_position,
        }
    }
    pub fn recover(self) -> Self {
        if self.store.health().state() != beryl_home_store::HomeHealthState::Failed {
            return self;
        }
        let Self {
            home,
            store,
            storage,
            state,
            faults,
            thread,
            item,
            session,
            content,
            intent,
            proof,
            asset,
            successor_position,
        } = self;
        let candidate = store.recover_same_home().unwrap();
        drop(storage);
        drop(state);
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let store = candidate.publish().unwrap();
        Self {
            home,
            store,
            storage,
            state,
            faults,
            thread,
            item,
            session,
            content,
            intent,
            proof,
            asset,
            successor_position,
        }
    }
}

fn seed_source(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    item: SyndicItemId,
    payload: &ComposerPayload,
    proof: Option<SealedAssetReferenceSetProof>,
) -> (ContentReference, ReplacementEditIntent) {
    let old = ordinary::current(storage, store, thread);
    let draft = old.draft();
    let turn = SyndicTurnId::from_bytes([203; 16]);
    let thread_revision = old.thread().revision().checked_next().unwrap();
    let digest = root_turn_chain_digest(turn);
    let selected = SelectedPathProof::new(Some(turn), thread_revision, digest);
    let projection = fixture_inline_paragraph_projection(item, turn, "historical");
    let revision = projection.revision();
    let (content, mut records) = support::composer_content_records(payload);
    let item_digest = fixture_advance_item_projection_digest(
        fixture_item_projection_digest_seed(),
        projection.id(),
        revision,
    );
    let entry = TranscriptViewEntryRecord::new(
        thread,
        TranscriptGeneration::FIRST,
        TranscriptPosition::FIRST,
        item,
        revision,
        ItemProjectionGeneration::FIRST,
        projection.id(),
        revision,
    );
    let transcript_digest =
        fixture_advance_transcript_digest(fixture_transcript_digest_seed(), &entry);
    let history = storage
        .history_summary(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    let history = HistorySummaryRecord::new(
        thread,
        history.revision().checked_next().unwrap(),
        thread_revision,
        Some(turn),
        digest,
        false,
        SyndicTimestamp::from_unix_millis(2),
    );
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        draft.id(),
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        None,
    );
    let execution = ThreadExecutionRecord::new(thread, support::exact_cas::execution_binding());
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);
    let maximum = proof
        .and_then(|p| p.sequential().maximum_image_label())
        .map_or(0, |v| v.get());
    records.extend([
        FixtureRecord::Thread(thread_record),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            draft.id(),
            draft.revision(),
            thread_revision,
        )),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            beryl_model::BindingRevision::new(1).unwrap(),
            selected,
            BindingState::unbound("replacement-source").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            beryl_model::BindingRevision::new(1).unwrap(),
            BindingLifecycle::Unbound,
            digest,
        )),
        FixtureRecord::ImageLabelAuthorityHead(
            ImageLabelAuthorityHeadV1::new(
                thread,
                2,
                ImageLabelFrontier::EMPTY,
                ImageLabelFrontier::from_raw(maximum),
            )
            .unwrap(),
        ),
        FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(thread, 2, ImageLabelFrontier::from_raw(maximum))
                .unwrap(),
        ),
        FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::FIRST,
            digest,
            SyndicTimestamp::from_unix_millis(2),
        )),
        FixtureRecord::TurnState(support::fixture_turn_state_with_capture(
            turn,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            1,
            1,
            1,
            1,
            0,
            SyndicTimestamp::from_unix_millis(2),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
                    TurnIncompleteReason::ItemAuditFailed,
                )),
            )
            .unwrap(),
        ),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            turn,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            proof,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
        FixtureRecord::Projection(projection.clone()),
        FixtureRecord::StableItemProjection(StableItemProjectionIndexRecord::new(
            item,
            ProjectionOrdinal::FIRST,
            projection.id(),
            revision,
        )),
        FixtureRecord::ItemProjectionSet(ItemProjectionSetRecord::new(
            item,
            ItemProjectionGeneration::FIRST,
            ProjectionFormatVersion::V1,
            revision,
            ProjectionTextSource::composer(content),
            10,
            1,
            0,
            item_digest,
            1,
            0,
            item_digest,
            MarkdownParserCheckpoint::new(
                content.summary().logical_utf8_bytes(),
                content.summary().logical_utf8_bytes(),
                ProjectionTextSourceCursor::Composer(
                    ContentPieceOrdinal::new(content.summary().piece_count() + 1).unwrap(),
                ),
                10,
                Box::<str>::default(),
                false,
                None,
            ),
            true,
        )),
        FixtureRecord::ItemProjectionHead(ItemProjectionHeadRecord::new(
            item,
            revision,
            revision,
            ItemProjectionGeneration::FIRST,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::TranscriptViewEntry(entry),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            TurnDepth::FIRST,
            turn,
            digest,
            TurnStateRevision::FIRST,
            TurnLifecycle::Incomplete,
            1,
            1,
            1,
            SyndicTimestamp::from_unix_millis(2),
        )),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            thread_revision,
            Some(turn),
            digest,
            1,
            1,
            transcript_digest,
            false,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            revision,
            1,
            Some(turn),
            digest,
            ProjectionLifecycle::Current,
        )),
    ]);
    support::commit(store, storage.clone(), support::batch(records));
    (
        content,
        ReplacementEditIntent::new(
            turn,
            selected,
            CurrentTranscriptEntryProof::new(
                TranscriptGeneration::FIRST,
                TranscriptPosition::FIRST,
            ),
        ),
    )
}
