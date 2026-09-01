use std::{num::NonZeroU64, path::PathBuf};

use beryl_home_store::{CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, AssetReferenceSetId, SyndicDraftMarkerId};
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, AppendAssetReferencePage, AssetMediaType, AssetOwner,
    AssetOwnerHeadUpdate, AssetReferencePageEntry, BeginAssetReferenceSet, PublishAssetMetadata,
    SealAssetReferenceSet, UpdateAssetOwnerHeads,
};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, SourcePosition, SourceRange, SourceRevision,
    SuccessorObject,
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
        assert!(atoms.iter().any(Result::is_err));
        let owned_atoms = atoms.to_vec();
        let owned_assets = assets.iter().map(|(asset, _)| *asset).collect::<Vec<_>>();
        let (kind, source_draft) =
            self.submit_via_composer(self.thread, move |host, home, binding| {
                commit_atoms(host, home, binding, &owned_atoms, &owned_assets)
            });
        let FirstAcceptanceKind::Idle { user_item_id } = kind else {
            panic!("fixture image submission expected an idle thread")
        };
        SubmittedTurn {
            turn: source_draft.submitted_turn_id(),
            user_item: user_item_id,
        }
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
            BeginAssetReferenceSet::new(beryl_state::AssetReferenceSetStagingAuthority::new(
                AssetReferenceSetId::from_bytes(*draft.as_bytes()),
                [draft.as_bytes()[0]; 32],
            ));
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
        let ordered_assets = build.ordered_assets();
        let seal = SealAssetReferenceSet::new(build, source, ordered_assets).unwrap();
        let proof = seal.sealed_proof();
        execute(
            home,
            assets.seal_reference_set(assets.revision(home).unwrap(), seal),
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
            BeginAssetReferenceSet::new(beryl_state::AssetReferenceSetStagingAuthority::new(
                AssetReferenceSetId::from_bytes(*draft.as_bytes()),
                [draft.as_bytes()[0]; 32],
            ));
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
        let ordered_assets = build.ordered_assets();
        let seal = SealAssetReferenceSet::new(build, source, ordered_assets).unwrap();
        let proof = seal.sealed_proof();
        execute(
            home,
            assets.seal_reference_set(assets.revision(home).unwrap(), seal),
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

fn commit_atoms(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    atoms: &[Result<&str, (ImageLabelOrdinal, usize)>],
    assets: &[AssetId],
) -> beryl_app::composer_host::ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        gpui_text_input::OperationId::new(1),
    );
    let origin = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(origin),
                SourceRange::new(origin, origin).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();

    let mut items = Vec::with_capacity(atoms.len());
    let mut metadata = Vec::new();
    let mut text_bytes = 0_u64;
    let mut line_count = 1_u64;
    let mut last_neighbor = None;
    for atom in atoms {
        match *atom {
            Ok(text) => {
                items.push(MutationPageItem::Utf8 {
                    inserted_offset: text_bytes,
                    text: text.into(),
                });
                text_bytes = text_bytes
                    .checked_add(u64::try_from(text.len()).unwrap())
                    .unwrap();
                line_count = line_count
                    .checked_add(
                        u64::try_from(text.bytes().filter(|byte| *byte == b'\n').count()).unwrap(),
                    )
                    .unwrap();
            }
            Err((label, asset_index)) => {
                let ordinal = u64::try_from(metadata.len() + 1).unwrap();
                let marker = marker_id(binding.candidate().draft_id(), ordinal);
                let object = InlineObjectId::new(u128::from_be_bytes(*marker.as_bytes()));
                let order = InlineObjectOrder::new(u128::from(ordinal));
                let asset = *assets
                    .get(asset_index)
                    .expect("image atom must select a published fixture asset");
                items.push(MutationPageItem::Object(
                    gpui_text_input::ObjectChange::Insert {
                        object: SuccessorObject::new(
                            object,
                            ByteOffset::new(text_bytes),
                            order,
                            17,
                            5,
                        ),
                    },
                ));
                metadata.push(
                    beryl_app::composer_host::ComposerHostImageMarkerMetadata::new(
                        object, label, asset,
                    ),
                );
                last_neighbor = Some(InlineObjectNeighbor::new(object, order));
            }
        }
    }
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        items,
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page),
        metadata.into_boxed_slice(),
    )
    .unwrap();
    let caret = SourcePosition::new(
        ByteOffset::new(text_bytes),
        last_neighbor.map_or(InlineObjectGap::NoObjects, InlineObjectGap::after),
    );
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            MutationStreamFinish {
                next_cursor: MutationCursor::new(0),
                next_ordinal: 0,
                cumulative_identity: MutationIdentity::ROOT,
                totals: MutationTotals::default(),
            },
            proposal_finish,
            LogicalExtent::new(text_bytes, line_count),
            MutationPositions::collapsed(caret),
        ),
    )
    .unwrap();
    for _ in 0..32 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(beryl_app::composer_host::ComposerHostMutationOutcome::Committed {
                binding,
                ..
            }) => return binding,
            Err(beryl_app::composer_host::ComposerHostError::MutationWorkPending) => {}
            other => panic!("image composer mutation did not commit: {other:?}"),
        }
    }
    panic!("image composer mutation did not converge")
}
