use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, MutationContribution};
use beryl_model::{
    DomainRevision, SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicItemId,
    SyndicThreadId, SyndicTurnId,
};

use crate::{
    AcceptedInputPromotionProof, AcceptedNextCandidate, ContentReference, SyndicMutationError,
    SyndicStorage, SyndicTimestamp, domain::SyndicDomain,
};

mod records;

use records::PromotionRecords;

/// Exact opaque next-turn authority plus caller-owned successor identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromoteAcceptedInput {
    candidate: AcceptedNextCandidate,
    successor_turn_id: SyndicTurnId,
    successor_item_id: SyndicItemId,
    promoted_at: SyndicTimestamp,
}

impl PromoteAcceptedInput {
    /// Combines one opaque earliest-candidate proof with fresh successor identities.
    #[must_use]
    pub const fn new(
        candidate: AcceptedNextCandidate,
        successor_turn_id: SyndicTurnId,
        successor_item_id: SyndicItemId,
        promoted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            candidate,
            successor_turn_id,
            successor_item_id,
            promoted_at,
        }
    }

    #[must_use]
    /// Returns the thread whose accepted input will be promoted.
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.candidate.thread_id()
    }

    #[must_use]
    /// Returns the permanent accepted-input identity.
    pub const fn accepted_input_id(&self) -> SyndicAcceptedInputId {
        self.candidate.input_id()
    }

    #[must_use]
    /// Returns the caller-owned fresh successor turn identity.
    pub const fn successor_turn_id(&self) -> SyndicTurnId {
        self.successor_turn_id
    }

    #[must_use]
    /// Returns the caller-owned fresh successor canonical-item identity.
    pub const fn successor_item_id(&self) -> SyndicItemId {
        self.successor_item_id
    }

    #[must_use]
    /// Returns the timestamp assigned to the promoted turn.
    pub const fn promoted_at(&self) -> SyndicTimestamp {
        self.promoted_at
    }

    #[must_use]
    /// Returns the exact domain revision fenced by the opaque candidate.
    pub const fn source_revision(&self) -> DomainRevision {
        self.candidate.source_revision()
    }

    #[must_use]
    /// Returns the sealed content reused by the successor item.
    pub const fn content(&self) -> ContentReference {
        self.candidate.basis().input().content()
    }

    #[must_use]
    /// Returns the sealed asset-set proof reused by the successor item.
    pub const fn asset_reference_set(&self) -> Option<SealedAssetReferenceSetProof> {
        self.candidate.basis().input().asset_reference_set()
    }

    pub(crate) const fn candidate(&self) -> &AcceptedNextCandidate {
        &self.candidate
    }

    pub(crate) const fn proof(&self) -> AcceptedInputPromotionProof {
        let basis = self.candidate.basis();
        AcceptedInputPromotionProof::new(
            basis.gate().revision(),
            crate::AcceptedRouteHeadProof::new(
                basis.generation().generation(),
                basis.generation().revision(),
            ),
            basis.leaf().revision(),
            self.successor_turn_id,
            self.successor_item_id,
            self.promoted_at,
        )
    }
}

/// Fixed-work reconciliation result for one accepted-input promotion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedInputPromotionStatus {
    /// The complete source authority remains exact and every successor identity is absent.
    Prior,
    /// The complete promotion successor and asset-independent storage witness are exact.
    Exact,
    /// Neither the complete prior nor complete exact successor shape is present.
    Collision,
}

impl SyndicStorage {
    /// Atomically promotes one exact earliest accepted input into a fresh pending ordinary turn.
    #[must_use]
    pub fn promote_accepted_input(&self, promotion: PromoteAcceptedInput) -> MutationContribution {
        self.handle.contribution(
            promotion.source_revision(),
            PromoteAcceptedInputMutation { promotion },
        )
    }
}

struct PromoteAcceptedInputMutation {
    promotion: PromoteAcceptedInput,
}

impl DomainMutation<SyndicDomain> for PromoteAcceptedInputMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        PromotionRecords::build(reader, &self.promotion).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        PromotionRecords::build(reader, &self.promotion)?.contribute(mutations)
    }
}
