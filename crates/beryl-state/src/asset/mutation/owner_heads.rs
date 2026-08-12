use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, MutationBuilder, PointReadLimit,
    ReconciliationReservation,
};
use beryl_model::SealedAssetReferenceSetProof;

use crate::RecordRevision;

use super::super::{
    ASSET_HEAD_LIMIT, ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES, AssetDomain, AssetMutationError,
    AssetOwner, AssetOwnerHeadExpectation, AssetOwnerHeadRecord, AssetOwnerHeadUpdateError,
    AssetOwnerHeadValidationError, AssetReferenceSetLifecycle, codec::AssetOwnerHeadCodec,
};
use super::require_manifest;

/// One exact optional-state assertion or transition in an atomic owner-head contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetOwnerHeadUpdate {
    owner: AssetOwner,
    expected: Option<AssetOwnerHeadExpectation>,
    replacement: Option<SealedAssetReferenceSetProof>,
    action: AssetOwnerHeadAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssetOwnerHeadAction {
    Assert,
    Replace,
}

impl AssetOwnerHeadUpdate {
    #[must_use]
    pub const fn replace(
        owner: AssetOwner,
        expected: Option<AssetOwnerHeadExpectation>,
        replacement: Option<SealedAssetReferenceSetProof>,
    ) -> Self {
        Self {
            owner,
            expected,
            replacement,
            action: AssetOwnerHeadAction::Replace,
        }
    }

    /// Asserts one exact optional head without changing its record.
    #[must_use]
    pub const fn assert(owner: AssetOwner, expected: Option<AssetOwnerHeadExpectation>) -> Self {
        Self {
            owner,
            expected,
            replacement: None,
            action: AssetOwnerHeadAction::Assert,
        }
    }

    #[must_use]
    pub const fn owner(self) -> AssetOwner {
        self.owner
    }

    #[must_use]
    pub const fn expected(self) -> Option<AssetOwnerHeadExpectation> {
        self.expected
    }

    #[must_use]
    pub const fn replacement(self) -> Option<SealedAssetReferenceSetProof> {
        self.replacement
    }

    const fn mutates(self) -> bool {
        matches!(self.action, AssetOwnerHeadAction::Replace)
            && (self.expected.is_some() || self.replacement.is_some())
    }
}

/// One exact optional owner-head state in a bounded validation-only participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetOwnerHeadAssertion {
    owner: AssetOwner,
    expected: Option<AssetOwnerHeadExpectation>,
}

impl AssetOwnerHeadAssertion {
    #[must_use]
    pub const fn new(owner: AssetOwner, expected: Option<AssetOwnerHeadExpectation>) -> Self {
        Self { owner, expected }
    }

    #[must_use]
    pub const fn owner(self) -> AssetOwner {
        self.owner
    }

    #[must_use]
    pub const fn expected(self) -> Option<AssetOwnerHeadExpectation> {
        self.expected
    }
}

/// Bounded exact owner-head assertions for one validation-only asset participant.
pub struct ValidateAssetOwnerHeads {
    assertions: Box<[AssetOwnerHeadAssertion]>,
}

impl ValidateAssetOwnerHeads {
    pub fn new(
        assertions: Box<[AssetOwnerHeadAssertion]>,
    ) -> Result<Self, AssetOwnerHeadValidationError> {
        if assertions.is_empty() {
            return Err(AssetOwnerHeadValidationError::Empty);
        }
        if assertions.len() > ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES {
            return Err(AssetOwnerHeadValidationError::TooMany {
                actual: assertions.len(),
            });
        }
        for (index, assertion) in assertions.iter().enumerate() {
            if assertions[..index]
                .iter()
                .any(|prior| prior.owner() == assertion.owner())
            {
                return Err(AssetOwnerHeadValidationError::DuplicateOwner(
                    assertion.owner(),
                ));
            }
        }
        Ok(Self { assertions })
    }

    #[must_use]
    pub fn assertions(&self) -> &[AssetOwnerHeadAssertion] {
        &self.assertions
    }
}

impl DomainValidator<AssetDomain> for ValidateAssetOwnerHeads {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        for assertion in &self.assertions {
            validate_expected_head(reader, assertion.owner, assertion.expected)?;
        }
        Ok(())
    }
}

/// Fixed-capacity owner-head assertion and publication participant for cross-domain home commands.
pub struct UpdateAssetOwnerHeads {
    updates: Box<[AssetOwnerHeadUpdate]>,
}

impl UpdateAssetOwnerHeads {
    pub fn new(updates: Box<[AssetOwnerHeadUpdate]>) -> Result<Self, AssetOwnerHeadUpdateError> {
        if updates.is_empty() {
            return Err(AssetOwnerHeadUpdateError::Empty);
        }
        if updates.len() > ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES {
            return Err(AssetOwnerHeadUpdateError::TooMany {
                actual: updates.len(),
            });
        }
        for (index, update) in updates.iter().enumerate() {
            if updates[..index]
                .iter()
                .any(|prior| prior.owner() == update.owner())
            {
                return Err(AssetOwnerHeadUpdateError::DuplicateOwner(update.owner()));
            }
        }
        if !updates.iter().any(|update| update.mutates()) {
            return Err(AssetOwnerHeadUpdateError::NoEffect);
        }
        Ok(Self { updates })
    }

    #[must_use]
    pub fn updates(&self) -> &[AssetOwnerHeadUpdate] {
        &self.updates
    }
}

impl DomainMutation<AssetDomain> for UpdateAssetOwnerHeads {
    type Error = AssetMutationError;

    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        for update in &self.updates {
            validate_head_update(reader, *update)?;
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        let count = self
            .updates
            .iter()
            .filter(|update| update.mutates())
            .count();
        reservation.reserve_records::<AssetOwnerHeadCodec>(count)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, AssetDomain>,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        for update in &self.updates {
            validate_head_update(reader, *update)?;
            if matches!(update.action, AssetOwnerHeadAction::Assert) {
                continue;
            }
            match (update.expected(), update.replacement()) {
                (None, None) => {}
                (None, Some(set)) => {
                    let owner = update.owner();
                    mutations.put::<AssetOwnerHeadCodec>(
                        &owner,
                        &AssetOwnerHeadRecord {
                            owner,
                            set,
                            owner_revision: RecordRevision::INITIAL,
                        },
                    )?;
                }
                (Some(expected), Some(replacement_set)) => {
                    let owner = update.owner();
                    let owner_revision = expected
                        .owner_revision()
                        .checked_next()
                        .map_err(|_| AssetMutationError::OwnerRevisionExhausted(owner))?;
                    mutations.put::<AssetOwnerHeadCodec>(
                        &owner,
                        &AssetOwnerHeadRecord {
                            owner,
                            set: replacement_set,
                            owner_revision,
                        },
                    )?;
                }
                (Some(_), None) => {
                    mutations.delete::<AssetOwnerHeadCodec>(&update.owner())?;
                }
            }
        }
        Ok(())
    }
}

fn validate_head_update(
    reader: &DomainReader<'_, AssetDomain>,
    update: AssetOwnerHeadUpdate,
) -> Result<(), AssetMutationError> {
    let owner = update.owner();
    validate_expected_head(reader, owner, update.expected())?;
    if matches!(update.action, AssetOwnerHeadAction::Assert) {
        return Ok(());
    }
    if let Some(replacement) = update.replacement() {
        require_sealed_proof(reader, replacement)?;
    }
    if update.expected().is_some() && update.replacement().is_some() {
        update
            .expected()
            .expect("present expectation was checked")
            .owner_revision()
            .checked_next()
            .map_err(|_| AssetMutationError::OwnerRevisionExhausted(owner))?;
    }
    Ok(())
}

fn validate_expected_head(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetOwner,
    expected: Option<AssetOwnerHeadExpectation>,
) -> Result<Option<AssetOwnerHeadRecord>, AssetMutationError> {
    let current = read_head(reader, owner)?;
    if current.as_ref().map(AssetOwnerHeadRecord::expectation) != expected {
        return Err(AssetMutationError::OwnerHeadMismatch(owner));
    }
    Ok(current)
}

fn require_sealed_proof(
    reader: &DomainReader<'_, AssetDomain>,
    proof: SealedAssetReferenceSetProof,
) -> Result<(), AssetMutationError> {
    let manifest = require_manifest(reader, proof.set_id())?;
    if manifest.lifecycle != AssetReferenceSetLifecycle::Sealed {
        return Err(AssetMutationError::ReferenceSetNotSealed(proof.set_id()));
    }
    if manifest.sealed_proof() != Some(proof) {
        return Err(AssetMutationError::BuildProofMismatch(proof.set_id()));
    }
    Ok(())
}

fn read_head(
    reader: &DomainReader<'_, AssetDomain>,
    owner: AssetOwner,
) -> Result<Option<AssetOwnerHeadRecord>, AssetMutationError> {
    reader
        .point::<AssetOwnerHeadCodec>(&owner, head_limit())
        .map_err(Into::into)
}

fn head_limit() -> PointReadLimit {
    PointReadLimit::new(ASSET_HEAD_LIMIT + 4).expect("head point bound is nonzero")
}
