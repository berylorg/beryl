use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use beryl_state::{AssetOwner, BerylState};
use syndic_storage::test_faults::FixtureBatch;
use syndic_storage::*;

#[path = "support/assets.rs"]
mod assets;
#[path = "support/records.rs"]
mod records;

use assets::admit_owner_set;
use records::{prepared_content_records, promotion_records};

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
        let source_draft = SyndicDraftId::from_bytes([seed.wrapping_add(2); 16]);
        let accepted_input = source_draft.accepted_input_id();
        let parent = SyndicTurnId::from_bytes([seed.wrapping_add(3); 16]);
        let accepted_marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(4); 16]);
        let draft_marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(5); 16]);

        let (accepted_prepared, draft_prepared) = match assets {
            FixtureAssets::MarkerFree => (
                composer_text("queued marker-free input"),
                composer_text("retained marker-free draft"),
            ),
            FixtureAssets::ImageBearing => (
                composer_image("queued image", accepted_marker, ImageLabelOrdinal::FIRST),
                composer_image(
                    "retained draft image",
                    draft_marker,
                    ImageLabelOrdinal::new(2).unwrap(),
                ),
            ),
        };
        let (accepted_content, accepted_content_records) =
            prepared_content_records(&accepted_prepared);
        let (draft_content, draft_content_records) = prepared_content_records(&draft_prepared);
        let empty_prepared = PreparedContent::composer(&ComposerPayload::default()).unwrap();
        let (_, empty_content_records) = prepared_content_records(&empty_prepared);

        let (accepted_proof, draft_proof) = match assets {
            FixtureAssets::MarkerFree => (None, None),
            FixtureAssets::ImageBearing => (
                Some(admit_owner_set(
                    &store,
                    state.assets(),
                    accepted_content,
                    accepted_marker,
                    ImageLabelOrdinal::FIRST,
                    AssetOwner::AcceptedInput(accepted_input),
                    seed.wrapping_add(20),
                    b"accepted image",
                )),
                Some(admit_owner_set(
                    &store,
                    state.assets(),
                    draft_content,
                    draft_marker,
                    ImageLabelOrdinal::new(2).unwrap(),
                    AssetOwner::CurrentDraft(current_draft),
                    seed.wrapping_add(21),
                    b"current draft image",
                )),
            ),
        };
        let mut records = promotion_records(
            thread,
            current_draft,
            source_draft,
            parent,
            execution_binding(seed),
            accepted_content,
            draft_content,
            accepted_proof,
            matches!(assets, FixtureAssets::ImageBearing),
        );
        records.extend(accepted_content_records);
        records.extend(draft_content_records);
        records.extend(empty_content_records);
        let mut batch = FixtureBatch::new();
        for record in records {
            batch.put(record).unwrap();
        }
        execute_one(
            &store,
            syndic.fixture_contribution(syndic.revision(&store).unwrap(), batch),
        );
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
        let candidate = self
            .syndic
            .accepted_next_candidate_page(&self.store, sources.records()[0], None, limits)
            .unwrap()
            .into_candidate()
            .expect("fixture owns one effective next-turn input");
        PromoteAcceptedInput::new(candidate, successor_turn, successor_item, time(20))
    }

    pub fn install_stray_owner(&self, owner: AssetOwner, seed: u8) {
        let marker = SyndicDraftMarkerId::from_bytes([seed.wrapping_add(1); 16]);
        let prepared = composer_image("stray", marker, ImageLabelOrdinal::FIRST);
        let content = prepared.reference(beryl_model::ContentRevision::new(1).unwrap());
        admit_owner_set(
            &self.store,
            self.state.assets(),
            content,
            marker,
            ImageLabelOrdinal::FIRST,
            owner,
            seed,
            b"stray owner",
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

fn composer_text(text: &str) -> PreparedContent {
    PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
    )
    .unwrap()
}

fn composer_image(
    text: &str,
    marker: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
) -> PreparedContent {
    PreparedContent::composer(
        &ComposerPayload::new(vec![
            ComposerAtom::text(text).unwrap(),
            ComposerAtom::image_marker(marker, label),
        ])
        .unwrap(),
    )
    .unwrap()
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
