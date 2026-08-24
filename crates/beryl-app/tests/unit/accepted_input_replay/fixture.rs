use std::num::NonZeroU64;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, SidecarByteLimit,
    SidecarNamespace,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId, SyndicThreadId,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferencePageEntry, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use syndic_storage::{
    AcceptedInputAdmission, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild,
    CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, IdleSubmission,
    ImageLabelOrdinal, PreparedContent, SyndicCurrentDraft, SyndicPointReadLimit, SyndicTimestamp,
};

use super::*;
use crate::input_admission::{build_accepted_input_command, idle_submission_command};

pub(super) struct Fixture {
    _directory: tempfile::TempDir,
    pub(super) store: HomeStore,
    pub(super) storage: SyndicStorage,
    pub(super) state: BerylState,
    thread: SyndicThreadId,
    seed: u8,
}

impl Fixture {
    pub(super) fn new(seed: u8) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn with_faults(
        seed: u8,
        faults: beryl_home_store::test_faults::FaultController,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults,
        )
        .unwrap();
        Self::from_store(seed, directory, store)
    }

    fn from_store(seed: u8, directory: tempfile::TempDir, mut store: HomeStore) -> Self {
        let storage = SyndicStorage::register(&mut store).unwrap();
        let state = BerylState::register(&mut store).unwrap();
        let thread = SyndicThreadId::from_bytes([seed; 16]);
        execute_one(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                    execution_binding(seed),
                    time(1),
                ),
            ),
        );
        Self {
            _directory: directory,
            store,
            storage,
            state,
            thread,
            seed,
        }
    }

    fn establish_pending_turn(&self) {
        self.publish_payload(
            &ComposerPayload::new(vec![ComposerAtom::text("initial turn").unwrap()]).unwrap(),
            2,
        );
        let current = self.current();
        let gate = self
            .storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let submission = IdleSubmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([self.seed.wrapping_add(2); 16]),
            SyndicItemId::from_bytes([self.seed.wrapping_add(3); 16]),
            None,
            time(3),
        );
        let command =
            idle_submission_command(&self.store, self.storage, self.state.assets(), submission)
                .unwrap();
        match self.store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("pending-turn fixture command committed with later failure: {outcome:?}"),
            CommandOutcome::NotCommitted { evidence } => {
                panic!("pending-turn fixture command was not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("pending-turn fixture command was indeterminate: {outcome:?}")
            }
        }
    }

    fn current(&self) -> SyndicCurrentDraft {
        self.storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    fn publish_payload(&self, payload: &ComposerPayload, updated_at: u64) {
        let prepared = PreparedContent::composer(payload).unwrap();
        execute_one(
            &self.store,
            self.storage.begin_content(
                self.storage.revision(&self.store).unwrap(),
                ContentBuild::from_prepared(&prepared),
            ),
        );
        let mut manifest = prepared.building_manifest();
        while let Some(append) = ContentAppend::prepare(&manifest, &prepared).unwrap() {
            manifest = append.next_manifest().clone();
            execute_one(
                &self.store,
                self.storage
                    .append_content(self.storage.revision(&self.store).unwrap(), append),
            );
        }
        let current = self.current();
        let DraftPayloadUpdateDecision::Update(update) =
            DraftPayloadUpdate::prepare(&current, &prepared, time(updated_at)).unwrap()
        else {
            panic!("fixture payload must change the current draft")
        };
        execute_one(
            &self.store,
            self.storage
                .update_draft_payload(self.storage.revision(&self.store).unwrap(), update),
        );
    }

    fn admit_current(
        &self,
        proof: Option<beryl_model::SealedAssetReferenceSetProof>,
        admitted_at: u64,
    ) -> AcceptedInputRecord {
        let current = self.current();
        let gate = self
            .storage
            .input_gate(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        let admission = AcceptedInputAdmission::new(
            self.thread,
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().content(),
            gate.revision(),
            SyndicDraftId::from_bytes([self.seed.wrapping_add(4); 16]),
            proof,
            time(admitted_at),
        );
        let input_id = admission.accepted_input_id();
        let command =
            build_accepted_input_command(&self.store, self.storage, self.state.assets(), admission)
                .unwrap();
        match self.store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("accepted-input fixture command committed with later failure: {outcome:?}"),
            CommandOutcome::NotCommitted { evidence } => {
                panic!("accepted-input fixture command was not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("accepted-input fixture command was indeterminate: {outcome:?}")
            }
        }
        self.storage
            .accepted_input(&self.store, input_id, point_limit())
            .unwrap()
            .unwrap()
    }

    pub(super) fn accept_text(&self, text: &str) -> AcceptedInputRecord {
        self.establish_pending_turn();
        self.publish_payload(
            &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
            4,
        );
        self.admit_current(None, 5)
    }

    pub(super) fn replay_factory(
        &self,
        record: AcceptedInputRecord,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<AcceptedInputReplayFactory, AcceptedInputReplayError> {
        let owner_head = self
            .state
            .assets()
            .owner_head(&self.store, AssetOwner::AcceptedInput(record.id()))
            .unwrap();
        AcceptedInputReplayFactory::prepare(
            &self.store,
            self.storage,
            self.state.assets(),
            AcceptedInputReplayContext::new(
                self.store.home_id(),
                self.store.health().generation().unwrap(),
                RuntimeMode::host(),
            ),
            record,
            owner_head,
            cancellation,
        )
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn recover_same_home(self) -> Self {
        let Self {
            _directory,
            store,
            storage: _,
            state: _,
            thread,
            seed,
        } = self;
        let candidate = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let store = candidate.publish();
        Self {
            _directory,
            store,
            storage,
            state,
            thread,
            seed,
        }
    }

    fn publish_asset(&self, bytes: &[u8]) -> AssetId {
        let sidecar = self
            .store
            .admit_sidecar(
                SidecarNamespace::new("images").unwrap(),
                bytes,
                SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap()),
            )
            .unwrap();
        let asset = AssetId::sha256_v1(
            sidecar.address().digest().as_bytes(),
            NonZeroU64::new(sidecar.address().length()).unwrap(),
        );
        let assets = self.state.assets();
        let revision = assets.revision(&self.store).unwrap();
        let metadata = assets
            .publish_metadata(
                revision,
                sidecar,
                PublishAssetMetadata::new(
                    asset,
                    AssetMediaType::new("image/png").unwrap(),
                    None,
                    revision.checked_next().unwrap(),
                ),
            )
            .unwrap();
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        metadata.add_to(&mut command).unwrap();
        match self.store.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("asset metadata fixture command committed with later failure: {outcome:?}"),
            CommandOutcome::NotCommitted { evidence } => {
                panic!("asset metadata fixture command was not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("asset metadata fixture command was indeterminate: {outcome:?}")
            }
        }
        asset
    }

    fn seal_repeated_reference_set(
        &self,
        marker_ids: [SyndicDraftMarkerId; 2],
        label: ImageLabelOrdinal,
        asset: AssetId,
    ) -> beryl_model::SealedAssetReferenceSetProof {
        let current = self.current();
        let source = current.draft().content().sealed_marker_summary().unwrap();
        let assets = self.state.assets();
        let begin = BeginAssetReferenceSet::new(
            AssetReferenceSetId::from_bytes([self.seed.wrapping_add(10); 16]),
            source.sequential(),
        );
        let staging = begin.staging_authority();
        execute_one(
            &self.store,
            assets.begin_reference_set(assets.revision(&self.store).unwrap(), begin),
        );
        let build = assets
            .staged_reference_set_manifest(&self.store, staging)
            .unwrap()
            .build_proof();
        execute_one(
            &self.store,
            assets.append_reference_page(
                assets.revision(&self.store).unwrap(),
                AppendAssetReferencePage::new(
                    build,
                    Box::new([
                        AssetReferencePageEntry::new(marker_ids[0], label, asset),
                        AssetReferencePageEntry::new(marker_ids[1], label, asset),
                    ]),
                )
                .unwrap(),
            ),
        );
        let build = assets
            .staged_reference_set_manifest(&self.store, staging)
            .unwrap()
            .build_proof();
        let ordered_assets = build.ordered_assets();
        let seal = SealAssetReferenceSet::new(build, source.sequential(), ordered_assets).unwrap();
        let proof = seal.sealed_proof();
        execute_one(
            &self.store,
            assets.seal_reference_set(assets.revision(&self.store).unwrap(), seal),
        );
        execute_one(
            &self.store,
            assets.update_owner_heads(
                assets.revision(&self.store).unwrap(),
                UpdateAssetOwnerHeads::new(
                    vec![AssetOwnerHeadUpdate::replace(
                        AssetOwner::CurrentDraft(current.draft().id()),
                        None,
                        Some(proof),
                    )]
                    .into_boxed_slice(),
                )
                .unwrap(),
            ),
        );
        proof
    }

    pub(super) fn accept_repeated_image(&self, leading_text: &str) -> AcceptedInputRecord {
        self.establish_pending_turn();
        let draft = self.current().draft().id();
        let marker_ids = [marker_id(draft, 1), marker_id(draft, 2)];
        let label = ImageLabelOrdinal::FIRST;
        let payload = ComposerPayload::new(vec![
            ComposerAtom::text(leading_text).unwrap(),
            ComposerAtom::image_marker(marker_ids[0], label),
            ComposerAtom::text(" between ").unwrap(),
            ComposerAtom::image_marker(marker_ids[1], label),
            ComposerAtom::text(" after").unwrap(),
        ])
        .unwrap();
        self.publish_payload(&payload, 4);
        let asset = self.publish_asset(b"\x89PNG\r\n\x1a\naccepted-input-replay");
        let proof = self.seal_repeated_reference_set(marker_ids, label, asset);
        self.admit_current(Some(proof), 5)
    }
}

pub(super) fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("fixture contribution command committed with later failure: {outcome:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("fixture contribution command was not committed: {evidence:?}")
        }
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("fixture contribution command was indeterminate: {outcome:?}")
        }
    }
}

fn marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
    let mut bytes = *draft.as_bytes();
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub(super) fn time(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

pub(super) fn execution_binding(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.wrapping_add(2); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl-accepted-input-replay",
        )
        .unwrap(),
    )
}
