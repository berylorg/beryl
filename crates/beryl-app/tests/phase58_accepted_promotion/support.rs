use beryl_home_store::{
    CommandCancellation, CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{AssetOwner, BerylState};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use super::{composer_support, publication_support};
use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostFlushAdmission, ComposerHostFlushAdvance,
    ComposerHostFlushCapture, ComposerHostFlushPurpose, ComposerHostSubmissionAdvance,
    ComposerHostSubmissionRequest, SyndicComposerHost,
};

#[derive(Clone, Copy)]
pub enum FixtureAssets {
    MarkerFree,
    ImageBearing,
}

pub struct Fixture {
    directory: tempfile::TempDir,
    pub store: HomeStore,
    pub syndic: SyndicStorage,
    pub state: BerylState,
    pub thread: SyndicThreadId,
    pub current_draft: SyndicDraftId,
    pub accepted_input: SyndicAcceptedInputId,
    pub parent: SyndicTurnId,
    pub accepted_proof: Option<beryl_model::SealedAssetReferenceSetProof>,
    pub draft_proof: Option<beryl_model::SealedAssetReferenceSetProof>,
    stray_proof: beryl_model::SealedAssetReferenceSetProof,
}

impl Fixture {
    pub fn new(seed: u8, assets: FixtureAssets) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(seed, assets, directory, store)
    }

    pub fn with_faults(
        seed: u8,
        assets: FixtureAssets,
        faults: beryl_home_store::test_faults::FaultController,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(seed, assets, directory, store)
    }

    fn from_store(
        seed: u8,
        assets: FixtureAssets,
        directory: tempfile::TempDir,
        mut store: HomeStore,
    ) -> Self {
        let state = BerylState::register(&mut store).unwrap();
        let syndic = SyndicStorage::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        let current_draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
        let initial_draft = SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]);
        let accepted_draft = SyndicDraftId::from_bytes([seed.wrapping_add(3); 16]);
        execute_one(
            &store,
            syndic.create_thread(
                syndic.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    initial_draft,
                    execution_binding(seed),
                    time(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );

        let (mut host, empty) =
            composer_support::activated(syndic, &store, thread, seed.wrapping_add(4), 1);
        let parent_binding =
            composer_support::commit_text(&mut host, &store, empty, 1, 0, 0, "parent", 6, 1);
        let parent = parent_binding.candidate().draft_id().submitted_turn_id();
        let first = submit(
            &mut host,
            &store,
            syndic,
            state.assets(),
            parent_binding,
            accepted_draft,
            seed.wrapping_add(5),
            None,
            time(5),
        );
        let FirstAcceptanceKind::Idle { user_item_id } = first else {
            panic!("fixture parent submission expected an idle thread")
        };

        let (mut host, empty) =
            composer_support::activated(syndic, &store, thread, seed.wrapping_add(6), 2);
        let accepted_binding = match assets {
            FixtureAssets::MarkerFree => composer_support::commit_text(
                &mut host,
                &store,
                empty,
                2,
                0,
                0,
                "queued marker-free input",
                24,
                1,
            ),
            FixtureAssets::ImageBearing => {
                let asset = publication_support::publish_image_asset(
                    &store,
                    state.assets(),
                    b"accepted image",
                );
                let (binding, _, _) = publication_support::insert_published_marker(
                    &mut host, &store, empty, 2, asset,
                );
                publication_support::insert_text_after_published_marker(
                    &mut host, &store, binding, 3,
                )
            }
        };
        let accepted_input = accepted_binding.candidate().draft_id().accepted_input_id();
        let marker_authority = matches!(assets, FixtureAssets::ImageBearing)
            .then(|| publication_support::authority(seed.wrapping_add(7)));
        let second = submit(
            &mut host,
            &store,
            syndic,
            state.assets(),
            accepted_binding,
            current_draft,
            seed.wrapping_add(8),
            marker_authority,
            time(10),
        );
        assert_eq!(second, FirstAcceptanceKind::Accepted);
        terminalize_parent_fixture(&store, syndic, thread, parent, user_item_id, time(15));
        let accepted_proof = state
            .assets()
            .owner_head(&store, AssetOwner::AcceptedInput(accepted_input))
            .unwrap()
            .map(|head| head.set());

        let (mut host, empty) =
            composer_support::activated(syndic, &store, thread, seed.wrapping_add(9), 4);
        let retained_binding = match assets {
            FixtureAssets::MarkerFree => composer_support::commit_text(
                &mut host,
                &store,
                empty,
                4,
                0,
                0,
                "retained marker-free draft",
                26,
                1,
            ),
            FixtureAssets::ImageBearing => {
                let asset = publication_support::publish_image_asset(
                    &store,
                    state.assets(),
                    b"current draft image",
                );
                let (binding, _, _) = publication_support::insert_published_marker(
                    &mut host, &store, empty, 4, asset,
                );
                publication_support::insert_text_after_published_marker(
                    &mut host, &store, binding, 5,
                )
            }
        };
        publish_current_draft(
            &mut host,
            &store,
            syndic,
            state.assets(),
            seed.wrapping_add(10),
            matches!(assets, FixtureAssets::ImageBearing)
                .then(|| publication_support::authority(seed.wrapping_add(11))),
            time(16),
        );
        assert_eq!(retained_binding.candidate().draft_id(), current_draft);
        let draft_proof = state
            .assets()
            .owner_head(&store, AssetOwner::CurrentDraft(current_draft))
            .unwrap()
            .map(|head| head.set());
        let stray_proof = create_stray_proof(&store, state.assets(), seed.wrapping_add(40));
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        Self {
            directory,
            store,
            syndic,
            state,
            thread,
            current_draft,
            accepted_input,
            parent,
            accepted_proof,
            draft_proof,
            stray_proof,
        }
    }

    pub fn promotion(&self, successor_seed: u8) -> PromoteAcceptedInput {
        self.promotion_with_ids(
            SyndicTurnId::from_bytes([successor_seed; 16]),
            SyndicItemId::from_bytes([successor_seed.wrapping_add(1); 16]),
        )
    }

    pub fn promotion_with_ids(
        &self,
        successor_turn: SyndicTurnId,
        successor_item: SyndicItemId,
    ) -> PromoteAcceptedInput {
        let revision = self.syndic.revision(&self.store).unwrap();
        let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
        let sources = self
            .syndic
            .accepted_next_source_page(&self.store, revision, None, limits)
            .unwrap();
        assert_eq!(sources.records().len(), 1);
        let page = self
            .syndic
            .accepted_next_candidate_page(&self.store, sources.records()[0], None, limits)
            .unwrap();
        let candidate = page.into_candidate().unwrap_or_else(|| {
            let gate = self
                .syndic
                .input_gate(&self.store, self.thread, point_limit())
                .unwrap();
            panic!("fixture owns one effective next-turn input: gate={gate:?}")
        });
        PromoteAcceptedInput::new(candidate, successor_turn, successor_item, time(20))
    }

    pub fn install_stray_owner(&self, owner: AssetOwner, seed: u8) {
        let previous = self
            .state
            .assets()
            .owner_head(&self.store, owner)
            .unwrap()
            .map(|head| head.expectation());
        execute_one(
            &self.store,
            self.state.assets().update_owner_heads(
                self.state.assets().revision(&self.store).unwrap(),
                beryl_state::UpdateAssetOwnerHeads::new(Box::from([
                    beryl_state::AssetOwnerHeadUpdate::replace(
                        owner,
                        previous,
                        Some(self.stray_proof),
                    ),
                ]))
                .unwrap(),
            ),
        );
        let _ = seed;
    }

    pub fn admit_later_input(&self, next_draft: SyndicDraftId, seed: u8) {
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        let initial_draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
        let queued_draft = SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]);
        execute_one(
            &self.store,
            self.syndic.create_thread(
                self.syndic.revision(&self.store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    initial_draft,
                    execution_binding(seed),
                    time(21),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
        let (mut host, empty) = composer_support::activated(
            self.syndic,
            &self.store,
            thread,
            seed.wrapping_add(3),
            seed.wrapping_add(4),
        );
        let binding = composer_support::commit_text(
            &mut host,
            &self.store,
            empty,
            40,
            0,
            0,
            "later parent",
            12,
            1,
        );
        assert!(matches!(
            submit(
                &mut host,
                &self.store,
                self.syndic,
                self.state.assets(),
                binding,
                queued_draft,
                seed.wrapping_add(5),
                None,
                time(22),
            ),
            FirstAcceptanceKind::Idle { .. }
        ));
        let (mut host, empty) = composer_support::activated(
            self.syndic,
            &self.store,
            thread,
            seed.wrapping_add(6),
            seed.wrapping_add(1),
        );
        let binding = composer_support::commit_text(
            &mut host,
            &self.store,
            empty,
            41,
            0,
            0,
            "later queued",
            12,
            1,
        );
        assert_eq!(
            submit(
                &mut host,
                &self.store,
                self.syndic,
                self.state.assets(),
                binding,
                next_draft,
                seed.wrapping_add(2),
                None,
                time(23),
            ),
            FirstAcceptanceKind::Accepted,
        );
    }

    pub fn recover_same_home(self) -> Self {
        let Self {
            directory,
            store,
            syndic: _,
            state: _,
            thread,
            current_draft,
            accepted_input,
            parent,
            accepted_proof,
            draft_proof,
            stray_proof,
        } = self;
        let candidate = store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let store = candidate.publish();
        Self {
            directory,
            store,
            syndic,
            state,
            thread,
            current_draft,
            accepted_input,
            parent,
            accepted_proof,
            draft_proof,
            stray_proof,
        }
    }
}

fn execution_binding(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.wrapping_add(6); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-phase58-promotion",
        )
        .unwrap(),
    )
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn submit(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: beryl_state::AssetState,
    binding: ComposerHostBinding,
    next_draft: SyndicDraftId,
    seed: u8,
    marker_authority: Option<beryl_app::composer_host::ComposerHostMarkerSealAuthority>,
    admitted_at: SyndicTimestamp,
) -> FirstAcceptanceKind {
    let seals = publication_support::service(store, syndic, assets, 1, 1);
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            next_draft,
            SyndicItemId::from_bytes([seed; 16]),
            DraftComposerMaterializationOperationIdV1::from_bytes([seed.wrapping_add(1); 16]),
            DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
            admitted_at,
            submission_admission_requirement(),
        ))
        .unwrap();
    assert_eq!(host.binding().unwrap(), binding);
    for _ in 0..128 {
        match host
            .advance_submission(
                store,
                ticket,
                assets,
                &seals,
                composer_support::operation_id(u64::from(seed) + 1_000),
                marker_authority,
                admitted_at,
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(_)
            | ComposerHostSubmissionAdvance::ReconciliationPending => {}
            ComposerHostSubmissionAdvance::ExactSuccess(kind) => return kind,
            outcome => panic!("fixture submission did not commit exactly: {outcome:?}"),
        }
    }
    panic!("fixture submission did not converge")
}

fn submission_admission_requirement() -> beryl_home_store::TurnStartAdmissionRequirement {
    beryl_app::cas_projection::ProjectionServiceConfig::try_new(
        1,
        4,
        beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
    )
    .unwrap()
    .turn_start_admission_requirement()
}

pub fn publish_current_draft(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: beryl_state::AssetState,
    seed: u8,
    marker_authority: Option<beryl_app::composer_host::ComposerHostMarkerSealAuthority>,
    published_at: SyndicTimestamp,
) {
    let seals = publication_support::service(store, syndic, assets, 1, 1);
    let flush = match host
        .begin_flush(ComposerHostFlushPurpose::Submission)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. } => ticket,
        outcome => panic!("fixture publication flush did not start: {outcome:?}"),
    };
    match host
        .capture_flush_publication(
            store,
            flush,
            assets,
            &seals,
            composer_support::operation_id(u64::from(seed) + 2_000),
            marker_authority,
            published_at,
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(_) => {}
        outcome => panic!("fixture publication was not captured: {outcome:?}"),
    }
    for _ in 0..128 {
        match host.advance_flush(store, flush).unwrap() {
            ComposerHostFlushAdvance::Progress(_)
            | ComposerHostFlushAdvance::ReconciliationPending => {}
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission) => return,
            outcome => panic!("fixture publication did not commit exactly: {outcome:?}"),
        }
    }
    panic!("fixture publication did not converge")
}

fn create_stray_proof(
    store: &HomeStore,
    assets: beryl_state::AssetState,
    seed: u8,
) -> beryl_model::SealedAssetReferenceSetProof {
    let staging = beryl_state::AssetReferenceSetStagingAuthority::new(
        beryl_model::AssetReferenceSetId::from_bytes([seed; 16]),
        [seed.wrapping_add(1); 32],
    );
    execute_one(
        store,
        assets.begin_reference_set(
            assets.revision(store).unwrap(),
            beryl_state::BeginAssetReferenceSet::new(staging),
        ),
    );
    let build = assets
        .staged_reference_set_manifest(store, staging)
        .unwrap()
        .build_proof();
    let source = beryl_model::SequentialMarkerSummaryV1::new(
        beryl_model::sequential_marker_digest_seed(),
        0,
        None,
    )
    .unwrap();
    let seal =
        beryl_state::SealAssetReferenceSet::new(build, source, build.ordered_assets()).unwrap();
    let proof = seal.sealed_proof();
    execute_one(
        store,
        assets.seal_reference_set(assets.revision(store).unwrap(), seal),
    );
    proof
}

pub fn terminalize_parent_fixture(
    store: &HomeStore,
    syndic: SyndicStorage,
    thread: SyndicThreadId,
    parent: SyndicTurnId,
    user_item: SyndicItemId,
    ended_at: SyndicTimestamp,
) {
    let thread_record = syndic.thread(store, thread, point_limit()).unwrap();
    let thread_record = thread_record.unwrap();
    let selected = thread_record.selected_path();
    assert_eq!(selected.tail(), Some(parent));
    let gate = syndic
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = syndic
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let history = syndic
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let activity = syndic
        .activity_query_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let status = TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap();
    let state = TurnStateRecord::new(
        parent,
        TurnStateRevision::FIRST,
        TurnLifecycle::Failed,
        1,
        0,
        Some(status),
        ended_at,
    )
    .unwrap();
    let generation = head.generation();
    let projection_revision = head.revision().checked_next().unwrap();
    let history = HistorySummaryRecord::new(
        thread,
        history.revision().checked_next().unwrap(),
        selected.thread_revision(),
        Some(parent),
        selected.digest(),
        true,
        ended_at,
    );
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::CanonicalItem(user_item))
        .unwrap();
    batch
        .delete(FixtureDelete::TurnItem {
            turn: parent,
            ordinal: TurnItemOrdinal::FIRST,
        })
        .unwrap();
    if let Some(source) = activity.source() {
        batch
            .delete(FixtureDelete::ActivityQuerySource {
                thread,
                work_period: activity.work_period(),
                source_thread: source.thread_id(),
                source_turn: source.turn_id(),
            })
            .unwrap();
    }
    for record in [
        FixtureRecord::TurnState(state),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                parent,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(status),
            )
            .unwrap(),
        ),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                gate.revision().checked_next().unwrap(),
                InputGateState::Idle,
                gate.accepted_high_water(),
                gate.route_generation_high_water(),
                None,
                gate.live_steering_count(),
                gate.live_next_turn_count(),
                gate.live_logical_utf8_bytes(),
            )
            .unwrap(),
        ),
        FixtureRecord::ActivityQueryHead(ActivityQueryHeadRecord::empty(thread)),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            generation,
            projection_revision,
            0,
            Some(parent),
            selected.digest(),
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread,
            generation,
            projection_revision,
            selected.thread_revision(),
            Some(parent),
            selected.digest(),
            1,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )),
        FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
            thread,
            generation,
            TurnDepth::FIRST,
            parent,
            selected.digest(),
            TurnStateRevision::FIRST,
            TurnLifecycle::Failed,
            1,
            0,
            0,
            ended_at,
        )),
    ] {
        batch.put(record).unwrap();
    }
    execute_one(
        store,
        syndic.fixture_contribution(syndic.revision(store).unwrap(), batch),
    );
}

fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed fixture mutation, got {outcome:?}")
        }
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("expected no later failure, got {outcome:?}"),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed fixture mutation, got {outcome:?}")
        }
    }
}
