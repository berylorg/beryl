use beryl_home_store::{CommandBuildError, HomeCommand, HomeStore, ReadError};
use beryl_state::{
    AssetOwner, AssetOwnerHeadAssertion, AssetOwnerHeadExpectation, AssetOwnerHeadUpdate,
    AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError, AssetReadError, AssetState,
    UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};
use syndic_storage::{
    AcceptedInputPromotionStatus, PromoteAcceptedInput, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage,
};

#[derive(Debug, thiserror::Error)]
pub enum InputAdmissionBuildError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Command(#[from] CommandBuildError),
    #[error(transparent)]
    AssetOwnerUpdate(#[from] AssetOwnerHeadUpdateError),
    #[error(transparent)]
    AssetOwnerValidation(#[from] AssetOwnerHeadValidationError),
    #[error(transparent)]
    AssetRead(#[from] AssetReadError),
    #[error(transparent)]
    SyndicRead(#[from] SyndicReadError),
    #[error("asset owner {0:?} has no durable head")]
    MissingOwnerHead(AssetOwner),
    #[error("asset owner head disagrees with the sealed proof supplied to Syndic")]
    OwnerHeadMismatch,
    #[error("asset destination owner {0:?} already has a durable head")]
    DestinationOwnerCollision(AssetOwner),
    #[error("relevant asset ownership changed during accepted-input promotion reconciliation")]
    ConcurrentPromotionReconciliation,
}

#[cfg(feature = "test-faults")]
pub fn accepted_input_promotion_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    promotion: PromoteAcceptedInput,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_accepted_input_promotion_command(store, syndic, assets, promotion)
}

#[cfg(not(feature = "test-faults"))]
pub(crate) fn accepted_input_promotion_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    promotion: PromoteAcceptedInput,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_accepted_input_promotion_command(store, syndic, assets, promotion)
}

fn build_accepted_input_promotion_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    promotion: PromoteAcceptedInput,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let proof = promotion.asset_reference_set();
    command_with_owner_transfer(
        store,
        assets,
        syndic.promote_accepted_input(promotion.clone()),
        proof,
        AssetOwner::AcceptedInput(promotion.accepted_input_id()),
        AssetOwner::SubmittedTurnItem(promotion.successor_item_id()),
    )
}

pub fn accepted_input_promotion_status(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    promotion: &PromoteAcceptedInput,
    limit: SyndicPointReadLimit,
) -> Result<AcceptedInputPromotionStatus, InputAdmissionBuildError> {
    let source = AssetOwner::AcceptedInput(promotion.accepted_input_id());
    let destination = AssetOwner::SubmittedTurnItem(promotion.successor_item_id());
    let observed = (
        assets.owner_head(store, source)?,
        assets.owner_head(store, destination)?,
    );
    let status = syndic.accepted_input_promotion_status(store, promotion, limit)?;
    let confirmed = (
        assets.owner_head(store, source)?,
        assets.owner_head(store, destination)?,
    );
    if observed != confirmed {
        return Err(InputAdmissionBuildError::ConcurrentPromotionReconciliation);
    }
    let (source_head, destination_head) = confirmed;
    let asset_status = match promotion.asset_reference_set() {
        Some(proof)
            if source_head
                .as_ref()
                .is_some_and(|head| head.owner() == source && head.set() == proof)
                && destination_head.is_none() =>
        {
            AcceptedInputPromotionStatus::Prior
        }
        Some(proof)
            if source_head.is_none()
                && destination_head
                    .as_ref()
                    .is_some_and(|head| head.owner() == destination && head.set() == proof) =>
        {
            AcceptedInputPromotionStatus::Exact
        }
        None if source_head.is_none() && destination_head.is_none() => status,
        Some(_) | None => AcceptedInputPromotionStatus::Collision,
    };
    Ok(if status == asset_status {
        status
    } else {
        AcceptedInputPromotionStatus::Collision
    })
}

fn command_with_owner_transfer(
    store: &HomeStore,
    assets: AssetState,
    syndic_contribution: beryl_home_store::MutationContribution,
    proof: Option<beryl_model::SealedAssetReferenceSetProof>,
    source: AssetOwner,
    destination: AssetOwner,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(syndic_contribution)?;
    let revision = assets.revision(store)?;
    if let Some(proof) = proof {
        let expected = exact_owner_head(store, assets, source, proof)?;
        require_absent_owner(store, assets, destination)?;
        command.add(
            assets.update_owner_heads(
                revision,
                UpdateAssetOwnerHeads::new(
                    vec![
                        AssetOwnerHeadUpdate::replace(source, Some(expected), None),
                        AssetOwnerHeadUpdate::replace(destination, None, Some(proof)),
                    ]
                    .into_boxed_slice(),
                )?,
            ),
        )?;
    } else {
        command.add_validation(
            assets.validate_owner_heads(
                revision,
                ValidateAssetOwnerHeads::new(
                    vec![
                        AssetOwnerHeadAssertion::new(source, None),
                        AssetOwnerHeadAssertion::new(destination, None),
                    ]
                    .into_boxed_slice(),
                )?,
            ),
        )?;
    }
    Ok(command)
}

fn exact_owner_head(
    store: &HomeStore,
    assets: AssetState,
    owner: AssetOwner,
    proof: beryl_model::SealedAssetReferenceSetProof,
) -> Result<AssetOwnerHeadExpectation, InputAdmissionBuildError> {
    let head = assets
        .owner_head(store, owner)?
        .ok_or(InputAdmissionBuildError::MissingOwnerHead(owner))?;
    if head.owner() != owner || head.set() != proof {
        return Err(InputAdmissionBuildError::OwnerHeadMismatch);
    }
    Ok(head.expectation())
}

fn require_absent_owner(
    store: &HomeStore,
    assets: AssetState,
    owner: AssetOwner,
) -> Result<(), InputAdmissionBuildError> {
    if assets.owner_head(store, owner)?.is_some() {
        return Err(InputAdmissionBuildError::DestinationOwnerCollision(owner));
    }
    Ok(())
}
