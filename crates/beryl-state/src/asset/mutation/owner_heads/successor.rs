use beryl_home_store::{
    FirstAcceptancePromotionProtocolV1, MutationBuildError, RecordCodec, SuccessorObservation,
    SuccessorPointRead, SuccessorPointReader, SuccessorPointRecord, SuccessorReadReservation,
    SuccessorWitness,
};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, FirstAcceptancePromotionSuccessorV1,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    SyndicAcceptedInputId, SyndicDraftId,
};

use crate::RecordRevision;

use super::{AssetOwnerHeadAction, AssetOwnerHeadUpdate};
use crate::asset::{
    AssetDomain, AssetOwner, AssetOwnerHeadRecord, AssetValidationError, codec::AssetOwnerHeadCodec,
};

#[derive(Clone, Copy)]
pub(super) struct FirstAcceptancePromotionWitnessV1 {
    draft_id: SyndicDraftId,
    accepted_input_id: SyndicAcceptedInputId,
    asset_reference_set: SealedAssetReferenceSetProof,
}

pub(super) fn first_acceptance_witness(
    updates: &[AssetOwnerHeadUpdate],
) -> Option<FirstAcceptancePromotionWitnessV1> {
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
    Some(FirstAcceptancePromotionWitnessV1 {
        draft_id,
        accepted_input_id,
        asset_reference_set: draft_set,
    })
}

struct PromotionOwnerHeadReadV1;

impl SuccessorPointRead<AssetDomain, FirstAcceptancePromotionProtocolV1>
    for PromotionOwnerHeadReadV1
{
    type Record = AssetOwnerHeadCodec;
    const MAX_DECODED_BYTES: usize = 512;

    fn derive_key(correlation: &FirstAcceptancePromotionSuccessorV1, ordinal: usize) -> AssetOwner {
        match ordinal {
            0 => AssetOwner::CurrentDraft(SyndicDraftId::from_bytes(
                *correlation.accepted_input_id().as_bytes(),
            )),
            1 => AssetOwner::AcceptedInput(correlation.accepted_input_id()),
            _ => AssetOwner::SubmittedTurnItem(correlation.submitted_item_id()),
        }
    }

    fn expected_value(
        correlation: &FirstAcceptancePromotionSuccessorV1,
        ordinal: usize,
    ) -> AssetOwnerHeadRecord {
        AssetOwnerHeadRecord {
            owner: Self::derive_key(correlation, ordinal),
            set: correlation
                .asset_reference_set()
                .unwrap_or_else(empty_reference_set),
            owner_revision: RecordRevision::INITIAL,
        }
    }
}

impl SuccessorWitness<AssetDomain, FirstAcceptancePromotionProtocolV1>
    for FirstAcceptancePromotionWitnessV1
{
    const MAX_RETAINED_BYTES: usize = 256;

    fn reserve_reads(
        &self,
        reservation: &mut SuccessorReadReservation<
            '_,
            AssetDomain,
            FirstAcceptancePromotionProtocolV1,
        >,
    ) -> Result<(), MutationBuildError> {
        reservation.reserve::<PromotionOwnerHeadReadV1>(3)
    }

    fn authenticate(
        &self,
        reader: &mut SuccessorPointReader<'_, AssetDomain, FirstAcceptancePromotionProtocolV1>,
    ) -> Result<SuccessorObservation<FirstAcceptancePromotionSuccessorV1>, AssetValidationError>
    {
        let draft = reader.read::<PromotionOwnerHeadReadV1>()?;
        let accepted = reader.read::<PromotionOwnerHeadReadV1>()?;
        let submitted = reader.read::<PromotionOwnerHeadReadV1>()?;
        let correlation = *reader.correlation();
        let exact_correlation = correlation.accepted_input_id() == self.accepted_input_id
            && SyndicDraftId::from_bytes(*correlation.accepted_input_id().as_bytes())
                == self.draft_id
            && correlation.asset_reference_set() == Some(self.asset_reference_set);
        let originals_absent = matches!(draft, SuccessorPointRecord::Absent)
            && matches!(accepted, SuccessorPointRecord::Absent);
        let submitted_exact = matches!(
            submitted,
            SuccessorPointRecord::Present(ref head)
                if head.owner() == AssetOwner::SubmittedTurnItem(correlation.submitted_item_id())
                    && head.set() == self.asset_reference_set
                    && head.owner_revision() == RecordRevision::INITIAL
        );
        if exact_correlation && originals_absent && submitted_exact {
            Ok(SuccessorObservation::Authenticated(correlation))
        } else {
            Ok(SuccessorObservation::Collision)
        }
    }
}

fn empty_reference_set() -> SealedAssetReferenceSetProof {
    let sequential = SequentialMarkerSummaryV1::new([0; 32], 0, None)
        .expect("zero-marker summary is structurally valid");
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([0; 16]),
        sequential,
        OrderedMarkerAssetSummaryV1::new([0; 32], 0),
        0,
        AssetReferenceSetDigest::from_bytes([0; 32]),
    )
    .expect("zero-marker proof is structurally valid")
}

const _: [(); 1] = [(); (AssetOwnerHeadCodec::MAX_KEY_BYTES <= 17) as usize];
