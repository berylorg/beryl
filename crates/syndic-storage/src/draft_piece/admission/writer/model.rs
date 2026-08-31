use std::num::NonZeroU64;

use sha2::{Digest, Sha256};

use crate::{
    DraftEditHistoryFrontierReferenceV1, DraftImageLabelProtectionHeadV1,
    DraftPieceRootReferenceV1, ImageLabelAuthorityHeadV1,
};

use super::super::*;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerLabelReadinessBindingV1 {
    digest: DraftMarkerAdmissionDigestV1,
    home_generation: NonZeroU64,
    owner: DraftMarkerAdmissionOwnerV1,
    label_authority: ImageLabelAuthorityHeadV1,
    protection: DraftImageLabelProtectionHeadV1,
    session_generation: NonZeroU64,
    predecessor_candidate_generation: u64,
    predecessor_root: DraftPieceRootReferenceV1,
    predecessor_history: DraftEditHistoryFrontierReferenceV1,
    disposition: DraftMarkerLabelReadinessDispositionV1,
    occurrence_commitment: DraftMarkerAdmissionDigestV1,
    sealed_target_root: DraftMarkerAdmissionRootV1,
    allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerWriterAdmissionV1 {
    binding: DraftMarkerLabelReadinessBindingV1,
    target_root: DraftMarkerAdmissionRootV1,
    remaining_count: u64,
}

impl DraftMarkerLabelReadinessBindingV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        home_generation: NonZeroU64,
        owner: DraftMarkerAdmissionOwnerV1,
        label_authority: ImageLabelAuthorityHeadV1,
        protection: DraftImageLabelProtectionHeadV1,
        session_generation: NonZeroU64,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        disposition: DraftMarkerLabelReadinessDispositionV1,
        occurrence_commitment: DraftMarkerAdmissionDigestV1,
        sealed_target_root: DraftMarkerAdmissionRootV1,
        allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
    ) -> Option<Self> {
        let mut value = Self {
            digest: DraftMarkerAdmissionDigestV1::from_bytes([0; 32]),
            home_generation,
            owner,
            label_authority,
            protection,
            session_generation,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            disposition,
            occurrence_commitment,
            sealed_target_root,
            allocation_range,
        };
        value.digest = value.recompute_digest();
        value.is_exact().then_some(value)
    }

    pub const fn digest(self) -> DraftMarkerAdmissionDigestV1 {
        self.digest
    }
    pub const fn home_generation(self) -> NonZeroU64 {
        self.home_generation
    }
    pub const fn owner(self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }
    pub const fn label_authority(self) -> ImageLabelAuthorityHeadV1 {
        self.label_authority
    }
    pub const fn protection(self) -> DraftImageLabelProtectionHeadV1 {
        self.protection
    }
    pub const fn session_generation(self) -> NonZeroU64 {
        self.session_generation
    }
    pub const fn predecessor_candidate_generation(self) -> u64 {
        self.predecessor_candidate_generation
    }
    pub const fn predecessor_root(self) -> DraftPieceRootReferenceV1 {
        self.predecessor_root
    }
    pub const fn predecessor_history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.predecessor_history
    }
    pub const fn disposition(self) -> DraftMarkerLabelReadinessDispositionV1 {
        self.disposition
    }
    pub const fn occurrence_commitment(self) -> DraftMarkerAdmissionDigestV1 {
        self.occurrence_commitment
    }
    pub const fn sealed_target_root(self) -> DraftMarkerAdmissionRootV1 {
        self.sealed_target_root
    }
    pub const fn allocation_range(self) -> Option<DraftMarkerLabelAllocationRangeV1> {
        self.allocation_range
    }

    pub(crate) fn is_exact(self) -> bool {
        self.label_authority.is_exact()
            && self.protection.is_exact()
            && self.label_authority.thread_id() == self.protection.thread_id()
            && self.owner.draft_id() == self.predecessor_root.key().draft_id()
            && self.predecessor_history.root() == self.predecessor_root
            && self.predecessor_history.candidate_generation()
                == self.predecessor_candidate_generation
            && self.sealed_target_root.tree() == DraftMarkerAdmissionTreeV1::TargetId
            && self.sealed_target_root.validate_shape().is_ok()
            && self.digest == self.recompute_digest()
    }

    fn recompute_digest(self) -> DraftMarkerAdmissionDigestV1 {
        let mut hasher = Sha256::new();
        hasher.update(b"syndic/draft-marker-writer-binding/v1");
        hasher.update(self.home_generation.get().to_be_bytes());
        hasher.update(self.owner.draft_id().as_bytes());
        hasher.update(self.owner.session_id().as_bytes());
        hasher.update(self.owner.operation_id().as_bytes());
        hasher.update(self.label_authority.digest());
        hasher.update(self.protection.digest());
        hasher.update(self.session_generation.get().to_be_bytes());
        hasher.update(self.predecessor_candidate_generation.to_be_bytes());
        hasher.update(self.predecessor_root.combined_digest().as_bytes());
        hasher.update(self.predecessor_history.digest().as_bytes());
        hasher.update([match self.disposition {
            DraftMarkerLabelReadinessDispositionV1::Reuse => 0,
            DraftMarkerLabelReadinessDispositionV1::Allocate => 1,
        }]);
        hasher.update(self.occurrence_commitment.as_bytes());
        hasher.update(self.sealed_target_root.digest().as_bytes());
        hasher.update(self.sealed_target_root.count().to_be_bytes());
        match self.allocation_range {
            Some(range) => {
                hasher.update([1]);
                hasher.update(range.first().get().to_be_bytes());
                hasher.update(range.last().get().to_be_bytes());
            }
            None => hasher.update([0]),
        }
        DraftMarkerAdmissionDigestV1::from_bytes(hasher.finalize().into())
    }
}

impl DraftMarkerWriterAdmissionV1 {
    pub(crate) fn new(binding: DraftMarkerLabelReadinessBindingV1) -> Option<Self> {
        let target_root = binding.sealed_target_root();
        let value = Self {
            binding,
            target_root,
            remaining_count: target_root.count(),
        };
        value.is_exact().then_some(value)
    }

    pub(crate) fn from_parts(
        binding: DraftMarkerLabelReadinessBindingV1,
        target_root: DraftMarkerAdmissionRootV1,
        remaining_count: u64,
    ) -> Option<Self> {
        let value = Self {
            binding,
            target_root,
            remaining_count,
        };
        value.is_exact().then_some(value)
    }

    pub const fn binding(self) -> DraftMarkerLabelReadinessBindingV1 {
        self.binding
    }
    pub const fn target_root(self) -> DraftMarkerAdmissionRootV1 {
        self.target_root
    }
    pub const fn remaining_count(self) -> u64 {
        self.remaining_count
    }
    pub const fn is_empty(self) -> bool {
        self.remaining_count == 0 && self.target_root.count() == 0
    }

    pub(crate) fn with_target_root(self, target_root: DraftMarkerAdmissionRootV1) -> Option<Self> {
        let remaining_count = self.remaining_count.checked_sub(1)?;
        Self::from_parts(self.binding, target_root, remaining_count)
    }

    pub(crate) fn is_exact(self) -> bool {
        self.binding.is_exact()
            && self.target_root.tree() == DraftMarkerAdmissionTreeV1::TargetId
            && self.target_root.validate_shape().is_ok()
            && self.target_root.count() == self.remaining_count
            && self.remaining_count <= self.binding.sealed_target_root().count()
    }
}
