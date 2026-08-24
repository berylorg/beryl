use std::{num::NonZeroU64, path::PathBuf};

use beryl_home_store::{CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, AssetReferenceSetId, SyndicDraftMarkerId};
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, AppendAssetReferencePage, AssetMediaType, AssetOwner,
    AssetOwnerHeadUpdate, AssetReferencePageEntry, BeginAssetReferenceSet, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};

use super::*;

impl Fixture {
    pub fn submit_repeated_image(&mut self) -> (SubmittedTurn, AssetId, PathBuf) {
        let label = ImageLabelOrdinal::FIRST;
        let (turn, mut assets) = self.submit_image_input(
            &[b"phase13 repeated image"],
            &[
                Ok("before "),
                Err((label, 0)),
                Ok(" middle "),
                Err((label, 0)),
                Ok(" after"),
            ],
        );
        let (asset, path) = assets
            .pop()
            .expect("repeated-image fixture must publish one asset");
        assert!(assets.is_empty());
        (turn, asset, path)
    }

    pub fn submit_image_input(
        &mut self,
        asset_bytes: &[&[u8]],
        atoms: &[Result<&str, (ImageLabelOrdinal, usize)>],
    ) -> (SubmittedTurn, Vec<(AssetId, PathBuf)>) {
        let assets = asset_bytes
            .iter()
            .map(|bytes| self.publish_asset_metadata(bytes))
            .collect::<Vec<_>>();
        let turn = self.submit_image_input_with_assets(&assets, atoms);
        (turn, assets)
    }

    pub fn submit_image_input_with_assets(
        &mut self,
        assets: &[(AssetId, PathBuf)],
        atoms: &[Result<&str, (ImageLabelOrdinal, usize)>],
    ) -> SubmittedTurn {
        let draft = {
            let command_home = self.store.live_home_command().unwrap();
            self.storage
                .current_draft(command_home.home(), self.thread, point_limit())
                .unwrap()
                .unwrap()
                .draft()
                .id()
        };
        let mut payload_atoms = Vec::with_capacity(atoms.len());
        let mut references = Vec::new();
        for atom in atoms {
            match *atom {
                Ok(text) => {
                    payload_atoms.push(ComposerAtom::text(text).unwrap());
                }
                Err((label, asset_index)) => {
                    let marker_id = marker_id(draft, u64::try_from(references.len() + 1).unwrap());
                    let (asset, _) = assets
                        .get(asset_index)
                        .expect("image atom must select a published fixture asset");
                    payload_atoms.push(ComposerAtom::image_marker(marker_id, label));
                    references.push(AssetReferencePageEntry::new(marker_id, label, *asset));
                }
            }
        }
        assert!(!references.is_empty());
        let payload = ComposerPayload::new(payload_atoms).unwrap();
        let base = self.prepare_submission_on(self.thread, payload, None);
        let proof = self.seal_reference_set(
            draft,
            base.expected_content()
                .sealed_marker_summary()
                .unwrap()
                .sequential(),
            references,
        );
        let submission = IdleSubmission::new(
            base.thread_id(),
            base.expected_thread_revision(),
            base.draft_id(),
            base.expected_draft_revision(),
            base.expected_content(),
            base.expected_gate_revision(),
            base.next_draft_id(),
            base.user_item_id(),
            Some(proof),
            base.admitted_at(),
        );
        let turn = submission.submitted_turn_id();
        let user_item = submission.user_item_id();
        self.store
            .execute_idle_submission(self.state.assets(), submission)
            .unwrap();
        SubmittedTurn { turn, user_item }
    }

    pub fn publish_asset_metadata(&mut self, bytes: &[u8]) -> (AssetId, PathBuf) {
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let sidecar = home
            .admit_sidecar(
                SidecarNamespace::new("images").unwrap(),
                bytes,
                SidecarByteLimit::new(NonZeroU64::new(1_024 * 1_024).unwrap()),
            )
            .unwrap();
        let path = sidecar.path().to_path_buf();
        let asset = AssetId::sha256_v1(
            sidecar.address().digest().as_bytes(),
            NonZeroU64::new(sidecar.address().length()).unwrap(),
        );
        let assets = self.state.assets();
        let revision = assets.revision(home).unwrap();
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
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        metadata.add_to(&mut command).unwrap();
        match home.execute(command) {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            CommandOutcome::NotCommitted { evidence } => {
                panic!("publish image metadata unexpectedly not committed: {evidence:?}")
            }
            outcome @ CommandOutcome::Committed {
                later_failure: Some(_),
                ..
            } => panic!("publish image metadata committed with later failure: {outcome:?}"),
            outcome @ CommandOutcome::Indeterminate { .. } => {
                panic!("publish image metadata indeterminate: {outcome:?}")
            }
        }
        (asset, path)
    }

    fn seal_reference_set(
        &self,
        draft: SyndicDraftId,
        source: beryl_model::SequentialMarkerSummaryV1,
        references: Vec<AssetReferencePageEntry>,
    ) -> SealedAssetReferenceSetProof {
        let assets = self.state.assets();
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let begin =
            BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes(*draft.as_bytes()), source);
        let staging = begin.staging_authority();
        execute(
            home,
            assets.begin_reference_set(assets.revision(home).unwrap(), begin),
        );
        for page in references.chunks(ASSET_REFERENCE_PAGE_MAX_ENTRIES) {
            let build = assets
                .staged_reference_set_manifest(home, staging)
                .unwrap()
                .build_proof();
            execute(
                home,
                assets.append_reference_page(
                    assets.revision(home).unwrap(),
                    AppendAssetReferencePage::new(build, page.to_vec().into_boxed_slice()).unwrap(),
                ),
            );
        }
        let build = assets
            .staged_reference_set_manifest(home, staging)
            .unwrap()
            .build_proof();
        let proof = build.sealed_proof().unwrap();
        execute(
            home,
            assets.seal_reference_set(
                assets.revision(home).unwrap(),
                SealAssetReferenceSet::new(build, source),
            ),
        );
        execute(
            home,
            assets.update_owner_heads(
                assets.revision(home).unwrap(),
                UpdateAssetOwnerHeads::new(
                    vec![AssetOwnerHeadUpdate::replace(
                        AssetOwner::CurrentDraft(draft),
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

    pub fn seal_repeated_asset_reference_set(
        &self,
        draft: SyndicDraftId,
        source: beryl_model::SequentialMarkerSummaryV1,
        marker_count: u64,
        asset: AssetId,
    ) -> SealedAssetReferenceSetProof {
        assert!(marker_count != 0);
        let assets = self.state.assets();
        let command_home = self.store.live_home_command().unwrap();
        let home = command_home.home();
        let begin =
            BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes(*draft.as_bytes()), source);
        let staging = begin.staging_authority();
        execute(
            home,
            assets.begin_reference_set(assets.revision(home).unwrap(), begin),
        );
        for first in (1..=marker_count).step_by(ASSET_REFERENCE_PAGE_MAX_ENTRIES) {
            let last = marker_count.min(
                first.saturating_add(u64::try_from(ASSET_REFERENCE_PAGE_MAX_ENTRIES).unwrap() - 1),
            );
            let page = (first..=last)
                .map(|ordinal| {
                    AssetReferencePageEntry::new(
                        marker_id(draft, ordinal),
                        ImageLabelOrdinal::new(ordinal).unwrap(),
                        asset,
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let build = assets
                .staged_reference_set_manifest(home, staging)
                .unwrap()
                .build_proof();
            execute(
                home,
                assets.append_reference_page(
                    assets.revision(home).unwrap(),
                    AppendAssetReferencePage::new(build, page).unwrap(),
                ),
            );
        }
        let build = assets
            .staged_reference_set_manifest(home, staging)
            .unwrap()
            .build_proof();
        let proof = build.sealed_proof().unwrap();
        execute(
            home,
            assets.seal_reference_set(
                assets.revision(home).unwrap(),
                SealAssetReferenceSet::new(build, source),
            ),
        );
        execute(
            home,
            assets.update_owner_heads(
                assets.revision(home).unwrap(),
                UpdateAssetOwnerHeads::new(
                    vec![AssetOwnerHeadUpdate::replace(
                        AssetOwner::CurrentDraft(draft),
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
}

pub(super) fn marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
    let mut bytes = *draft.as_bytes();
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}
