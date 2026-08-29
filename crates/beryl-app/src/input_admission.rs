use beryl_home_store::{CommandBuildError, HomeCommand, HomeStore, ReadError};
use beryl_state::{
    AssetOwner, AssetOwnerHeadAssertion, AssetOwnerHeadExpectation, AssetOwnerHeadRecord,
    AssetOwnerHeadUpdate, AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError, AssetReadError,
    AssetState, UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};
use syndic_storage::{
    AcceptedInputPromotionStatus, FirstAcceptance, FirstAcceptanceKind, FirstAcceptanceStatus,
    PromoteAcceptedInput, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
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
    #[error("relevant asset ownership changed during first-acceptance reconciliation")]
    ConcurrentFirstAcceptanceReconciliation,
    #[error("the first-acceptance identity collides with durable state")]
    FirstAcceptanceCollision,
}

pub enum FirstAcceptanceCommand {
    Execute(HomeCommand),
    AlreadyAccepted(FirstAcceptanceKind),
}

#[cfg(feature = "test-faults")]
pub fn first_acceptance_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    acceptance: FirstAcceptance,
) -> Result<FirstAcceptanceCommand, InputAdmissionBuildError> {
    build_first_acceptance_command(store, syndic, assets, acceptance)
}

#[cfg(not(feature = "test-faults"))]
pub(crate) fn first_acceptance_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    acceptance: FirstAcceptance,
) -> Result<FirstAcceptanceCommand, InputAdmissionBuildError> {
    build_first_acceptance_command(store, syndic, assets, acceptance)
}

fn build_first_acceptance_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    acceptance: FirstAcceptance,
) -> Result<FirstAcceptanceCommand, InputAdmissionBuildError> {
    let source = AssetOwner::CurrentDraft(acceptance.draft_id());
    let destination = first_acceptance_destination(&acceptance);
    let proof = acceptance.asset_reference_set();
    match first_acceptance_status_for(store, syndic, assets, &acceptance, admission_point_limit())?
    {
        FirstAcceptanceStatus::ExactNew(kind) => {
            return Ok(FirstAcceptanceCommand::AlreadyAccepted(kind));
        }
        FirstAcceptanceStatus::Collision => {
            return Err(InputAdmissionBuildError::FirstAcceptanceCollision);
        }
        FirstAcceptanceStatus::ExactOld => {}
    }
    let syndic_revision = syndic.revision(store)?;
    command_with_owner_transfer(
        store,
        assets,
        syndic.first_acceptance(syndic_revision, acceptance),
        proof,
        source,
        destination,
    )
    .map(FirstAcceptanceCommand::Execute)
}

pub fn first_acceptance_status(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    acceptance: &FirstAcceptance,
    limit: SyndicPointReadLimit,
) -> Result<FirstAcceptanceStatus, InputAdmissionBuildError> {
    first_acceptance_status_for(store, syndic, assets, acceptance, limit)
}

fn first_acceptance_status_for(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    acceptance: &FirstAcceptance,
    limit: SyndicPointReadLimit,
) -> Result<FirstAcceptanceStatus, InputAdmissionBuildError> {
    let observed =
        FirstAcceptanceCrossDomainObservation::read(store, syndic, assets, acceptance, limit)?;
    let confirmed =
        FirstAcceptanceCrossDomainObservation::read(store, syndic, assets, acceptance, limit)?;
    if observed != confirmed {
        return Err(InputAdmissionBuildError::ConcurrentFirstAcceptanceReconciliation);
    }
    let source = AssetOwner::CurrentDraft(acceptance.draft_id());
    let destination = first_acceptance_destination(acceptance);
    let status = confirmed.status;
    let asset_status = match acceptance.asset_reference_set() {
        Some(proof)
            if confirmed
                .source
                .as_ref()
                .is_some_and(|head| head.owner() == source && head.set() == proof)
                && confirmed.destination.is_none() =>
        {
            FirstAcceptanceStatus::ExactOld
        }
        Some(proof)
            if confirmed.source.is_none()
                && confirmed
                    .destination
                    .as_ref()
                    .is_some_and(|head| head.owner() == destination && head.set() == proof) =>
        {
            FirstAcceptanceStatus::ExactNew(first_acceptance_kind(acceptance))
        }
        None if confirmed.source.is_none() && confirmed.destination.is_none() => status,
        Some(_) | None => FirstAcceptanceStatus::Collision,
    };
    Ok(if status == asset_status {
        status
    } else {
        FirstAcceptanceStatus::Collision
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FirstAcceptanceCrossDomainObservation {
    status: FirstAcceptanceStatus,
    source: Option<AssetOwnerHeadRecord>,
    destination: Option<AssetOwnerHeadRecord>,
}

impl FirstAcceptanceCrossDomainObservation {
    fn read(
        store: &HomeStore,
        syndic: &SyndicStorage,
        assets: &AssetState,
        acceptance: &FirstAcceptance,
        limit: SyndicPointReadLimit,
    ) -> Result<Self, InputAdmissionBuildError> {
        let status = syndic.first_acceptance_status(store, acceptance, limit)?;
        Ok(Self {
            status,
            source: assets.owner_head(store, AssetOwner::CurrentDraft(acceptance.draft_id()))?,
            destination: assets.owner_head(store, first_acceptance_destination(acceptance))?,
        })
    }
}

fn first_acceptance_kind(acceptance: &FirstAcceptance) -> FirstAcceptanceKind {
    if matches!(
        acceptance.expected_gate_state(),
        syndic_storage::InputGateState::Idle
    ) {
        FirstAcceptanceKind::Idle {
            user_item_id: acceptance.idle_user_item_id(),
        }
    } else {
        FirstAcceptanceKind::Accepted
    }
}

fn first_acceptance_destination(acceptance: &FirstAcceptance) -> AssetOwner {
    match first_acceptance_kind(acceptance) {
        FirstAcceptanceKind::Idle { user_item_id } => AssetOwner::SubmittedTurnItem(user_item_id),
        FirstAcceptanceKind::Accepted => AssetOwner::AcceptedInput(acceptance.accepted_input_id()),
    }
}

fn admission_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(64 * 1024).expect("admission point limit is nonzero")
}

#[cfg(feature = "test-faults")]
pub fn accepted_input_promotion_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    promotion: PromoteAcceptedInput,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_accepted_input_promotion_command(store, syndic, assets, promotion)
}

#[cfg(not(feature = "test-faults"))]
pub(crate) fn accepted_input_promotion_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
    promotion: PromoteAcceptedInput,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_accepted_input_promotion_command(store, syndic, assets, promotion)
}

fn build_accepted_input_promotion_command(
    store: &HomeStore,
    syndic: &SyndicStorage,
    assets: &AssetState,
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
    syndic: &SyndicStorage,
    assets: &AssetState,
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
    assets: &AssetState,
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
    assets: &AssetState,
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
    assets: &AssetState,
    owner: AssetOwner,
) -> Result<(), InputAdmissionBuildError> {
    if assets.owner_head(store, owner)?.is_some() {
        return Err(InputAdmissionBuildError::DestinationOwnerCollision(owner));
    }
    Ok(())
}
