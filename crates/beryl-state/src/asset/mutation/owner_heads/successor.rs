use beryl_home_store::{
    FirstAcceptancePromotionAssetAdapter, FirstAcceptancePromotionAssetPlan,
    FirstAcceptancePromotionAssetSeed, RecordCodec,
};
use beryl_model::FirstAcceptancePromotionSuccessorV1;

use crate::RecordRevision;

use super::{AssetOwnerHeadAction, AssetOwnerHeadUpdate};
use crate::asset::{AssetDomain, AssetOwner, AssetOwnerHeadRecord, codec::AssetOwnerHeadCodec};

#[derive(Clone, Copy)]
pub(super) struct PromotionOwnerHeadAdapterV1;

pub(super) fn first_acceptance_promotion_seed(
    updates: &[AssetOwnerHeadUpdate],
) -> Option<FirstAcceptancePromotionAssetSeed> {
    if updates.len() != 2 {
        return None;
    }
    let mut draft = None;
    let mut accepted = None;
    for update in updates {
        match (update.owner(), update.action) {
            (AssetOwner::CurrentDraft(draft_id), AssetOwnerHeadAction::Replace)
                if update.replacement().is_none() =>
            {
                let expected = update.expected()?;
                draft = Some((draft_id, expected.set()));
            }
            (AssetOwner::AcceptedInput(input_id), AssetOwnerHeadAction::Replace)
                if update.expected().is_none() =>
            {
                accepted = Some((input_id, update.replacement()?));
            }
            _ => return None,
        }
    }
    let ((draft_id, draft_set), (accepted_input_id, accepted_set)) = (draft?, accepted?);
    if draft_id.accepted_input_id() != accepted_input_id || draft_set != accepted_set {
        return None;
    }
    Some(FirstAcceptancePromotionAssetSeed {
        draft_id,
        accepted_input_id,
        asset_reference_set: draft_set,
    })
}

impl FirstAcceptancePromotionAssetAdapter<AssetDomain> for PromotionOwnerHeadAdapterV1 {
    type OwnerHead = AssetOwnerHeadCodec;
    const MAX_DECODED_BYTES: usize = 512;

    fn derive_plan(
        seed: &FirstAcceptancePromotionAssetSeed,
        correlation: &FirstAcceptancePromotionSuccessorV1,
    ) -> Option<FirstAcceptancePromotionAssetPlan<AssetOwner, AssetOwnerHeadRecord>> {
        if correlation.accepted_input_id() != seed.accepted_input_id
            || correlation.asset_reference_set() != Some(seed.asset_reference_set)
        {
            return None;
        }
        let submitted = AssetOwner::SubmittedTurnItem(correlation.submitted_item_id());
        Some(FirstAcceptancePromotionAssetPlan {
            original_draft: AssetOwner::CurrentDraft(seed.draft_id),
            original_accepted: AssetOwner::AcceptedInput(seed.accepted_input_id),
            submitted,
            expected_submitted: AssetOwnerHeadRecord {
                owner: submitted,
                set: seed.asset_reference_set,
                owner_revision: RecordRevision::INITIAL,
            },
        })
    }
}

const _: [(); 1] = [(); (AssetOwnerHeadCodec::MAX_KEY_BYTES <= 17) as usize];
