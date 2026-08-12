use beryl_home_store::{
    CommandBuildError, CursorReadLimits, HomeCommand, HomeGeneration, HomeHealthState, HomeStore,
    ReadError,
};
use beryl_model::BerylHomeId;
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, ASSET_REFERENCE_PAGE_MAX_STORED_BYTES, AssetLabelDisposition,
    AssetOwner, AssetOwnerHeadAssertion, AssetOwnerHeadExpectation, AssetOwnerHeadUpdate,
    AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError, AssetReadError, AssetState,
    UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};
use syndic_storage::{
    AcceptedInputAdmission, AcceptedInputPromotionStatus, CancelReplacementEdit, IdleSubmission,
    PromoteAcceptedInput, StartReplacementEdit, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage,
};

const LABEL_AUTHORITY_POINT_MAX_STORED_BYTES: usize = 8 * 1024;

/// Failure while composing one exact cross-domain input command.
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
    #[error("the input thread is unavailable while proving historical image labels")]
    LabelThreadUnavailable,
    #[error("image label {label} is reserved but has no admitted origin asset")]
    ReservedImageLabel {
        label: beryl_model::ImageLabelOrdinal,
    },
    #[error("image label {label} disagrees with its admitted origin asset")]
    HistoricalImageLabelMismatch {
        label: beryl_model::ImageLabelOrdinal,
    },
    #[error("asset reference paging reported more records without a continuation ordinal")]
    AssetReferencePageStalled,
    #[error("accepted-input preparation requires a healthy home, got {state:?}")]
    HomeNotHealthy { state: HomeHealthState },
    #[error("relevant asset ownership changed during accepted-input promotion reconciliation")]
    ConcurrentPromotionReconciliation,
}

/// Opaque exact accepted-input publication prepared for its owning projection service.
pub struct PreparedAcceptedInputAdmission {
    pub(crate) command: HomeCommand,
    pub(crate) admission: AcceptedInputAdmission,
    pub(crate) home_id: BerylHomeId,
    pub(crate) home_generation: HomeGeneration,
}

impl PreparedAcceptedInputAdmission {
    #[must_use]
    pub const fn accepted_input_id(&self) -> beryl_model::SyndicAcceptedInputId {
        self.admission.accepted_input_id()
    }

    #[must_use]
    pub const fn thread_id(&self) -> beryl_model::SyndicThreadId {
        self.admission.thread_id()
    }
}

/// Builds one atomic idle-submission command with its compact asset-owner transition.
///
/// Production callers must use the projection service's final gated execution boundary.
#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub fn idle_submission_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    submission: IdleSubmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_idle_submission_command(store, syndic, assets, submission)
}

#[cfg(not(feature = "test-faults"))]
pub(crate) fn idle_submission_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    submission: IdleSubmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    build_idle_submission_command(store, syndic, assets, submission)
}

fn build_idle_submission_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    submission: IdleSubmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let proof = submission.asset_reference_set();
    validate_historical_image_labels(store, syndic, assets, submission.thread_id(), proof)?;
    let source = AssetOwner::CurrentDraft(submission.draft_id());
    let destination = AssetOwner::SubmittedTurnItem(submission.user_item_id());
    command_with_owner_transfer(
        store,
        assets,
        syndic.submit_idle_draft(syndic.revision(store)?, submission),
        proof,
        source,
        destination,
    )
}

/// Builds one atomic accepted-input promotion and asset-owner transfer.
#[cfg(feature = "test-faults")]
#[doc(hidden)]
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
    let source = AssetOwner::AcceptedInput(promotion.accepted_input_id());
    let destination = AssetOwner::SubmittedTurnItem(promotion.successor_item_id());
    command_with_owner_transfer(
        store,
        assets,
        syndic.promote_accepted_input(promotion),
        proof,
        source,
        destination,
    )
}

/// Reconciles both Syndic promotion state and its atomic asset-owner transfer.
pub fn accepted_input_promotion_status(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    promotion: &PromoteAcceptedInput,
    limit: SyndicPointReadLimit,
) -> Result<AcceptedInputPromotionStatus, InputAdmissionBuildError> {
    let source = AssetOwner::AcceptedInput(promotion.accepted_input_id());
    let destination = AssetOwner::SubmittedTurnItem(promotion.successor_item_id());
    let observed_asset_heads = (
        assets.owner_head(store, source)?,
        assets.owner_head(store, destination)?,
    );
    let status = syndic.accepted_input_promotion_status(store, promotion, limit)?;
    let confirmed_asset_heads = (
        assets.owner_head(store, source)?,
        assets.owner_head(store, destination)?,
    );
    if observed_asset_heads != confirmed_asset_heads {
        return Err(InputAdmissionBuildError::ConcurrentPromotionReconciliation);
    }
    let (source_head, destination_head) = confirmed_asset_heads;
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

/// Prepares one atomic accepted-input publication for its owning projection service.
pub fn prepare_accepted_input_admission(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    admission: AcceptedInputAdmission,
) -> Result<PreparedAcceptedInputAdmission, InputAdmissionBuildError> {
    let health = store.health();
    let Some(home_generation) = (health.state() == HomeHealthState::Healthy)
        .then(|| health.generation())
        .flatten()
    else {
        return Err(InputAdmissionBuildError::HomeNotHealthy {
            state: health.state(),
        });
    };
    let home_id = store.home_id();
    let command = build_accepted_input_command(store, syndic, assets, admission.clone())?;
    Ok(PreparedAcceptedInputAdmission {
        command,
        admission,
        home_id,
        home_generation,
    })
}

pub(crate) fn build_accepted_input_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    admission: AcceptedInputAdmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let proof = admission.asset_reference_set();
    validate_historical_image_labels(store, syndic, assets, admission.thread_id(), proof)?;
    let source = AssetOwner::CurrentDraft(admission.draft_id());
    let destination = AssetOwner::AcceptedInput(admission.accepted_input_id());
    command_with_owner_transfer(
        store,
        assets,
        syndic.admit_accepted_input(syndic.revision(store)?, admission),
        proof,
        source,
        destination,
    )
}

/// Builds one atomic replacement-start command and duplicates target asset ownership.
pub fn start_replacement_edit_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    edit: StartReplacementEdit,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let proof = edit.asset_reference_set();
    let historical_owner = AssetOwner::SubmittedTurnItem(edit.target_item_id());
    let draft_owner = AssetOwner::CurrentDraft(edit.draft_id());
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(syndic.start_replacement_edit(syndic.revision(store)?, edit))?;
    let revision = assets.revision(store)?;
    if let Some(proof) = proof {
        let historical = exact_owner_head(store, assets, historical_owner, proof)?;
        require_absent_owner(store, assets, draft_owner)?;
        command.add(
            assets.update_owner_heads(
                revision,
                UpdateAssetOwnerHeads::new(
                    vec![
                        AssetOwnerHeadUpdate::assert(historical_owner, Some(historical)),
                        AssetOwnerHeadUpdate::replace(draft_owner, None, Some(proof)),
                    ]
                    .into_boxed_slice(),
                )?,
            ),
        )?;
    } else {
        add_absent_owner_validation(
            &mut command,
            assets,
            revision,
            historical_owner,
            draft_owner,
        )?;
    }
    Ok(command)
}

/// Builds one Syndic-only replacement cancellation command.
pub fn cancel_replacement_edit_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    cancellation: CancelReplacementEdit,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(syndic.cancel_replacement_edit(syndic.revision(store)?, cancellation))?;
    Ok(command)
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
        add_absent_owner_validation(&mut command, assets, revision, source, destination)?;
    }
    Ok(command)
}

fn add_absent_owner_validation(
    command: &mut HomeCommand,
    assets: AssetState,
    revision: beryl_model::DomainRevision,
    first: AssetOwner,
    second: AssetOwner,
) -> Result<(), InputAdmissionBuildError> {
    command.add_validation(
        assets.validate_owner_heads(
            revision,
            ValidateAssetOwnerHeads::new(
                vec![
                    AssetOwnerHeadAssertion::new(first, None),
                    AssetOwnerHeadAssertion::new(second, None),
                ]
                .into_boxed_slice(),
            )?,
        ),
    )?;
    Ok(())
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

fn validate_historical_image_labels(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    thread_id: beryl_model::SyndicThreadId,
    proof: Option<beryl_model::SealedAssetReferenceSetProof>,
) -> Result<(), InputAdmissionBuildError> {
    let Some(proof) = proof else {
        return Ok(());
    };
    let point_limit = SyndicPointReadLimit::new(LABEL_AUTHORITY_POINT_MAX_STORED_BYTES)
        .expect("label-authority point bound is nonzero");
    let thread = syndic
        .thread(store, thread_id, point_limit)?
        .ok_or(InputAdmissionBuildError::LabelThreadUnavailable)?;
    let current = thread.image_label_frontiers().current();
    let page_limits = CursorReadLimits::new(
        ASSET_REFERENCE_PAGE_MAX_ENTRIES,
        ASSET_REFERENCE_PAGE_MAX_STORED_BYTES,
    )
    .expect("asset reference page bounds are nonzero");
    let mut after = None;
    loop {
        let page = assets.reference_set_entries(store, proof, after, page_limits)?;
        for entry in page.records() {
            if entry.label_disposition() != AssetLabelDisposition::First
                || !current.contains(entry.label())
            {
                continue;
            }
            let origin = syndic
                .resolve_image_label_origin_span(store, thread_id, entry.label(), point_limit)?
                .ok_or(InputAdmissionBuildError::ReservedImageLabel {
                    label: entry.label(),
                })?;
            let first = assets
                .label_first_reference(store, origin.span().asset_reference_set(), entry.label())?
                .ok_or(InputAdmissionBuildError::ReservedImageLabel {
                    label: entry.label(),
                })?;
            if first.label() != entry.label() || first.asset_id() != entry.asset_id() {
                return Err(InputAdmissionBuildError::HistoricalImageLabelMismatch {
                    label: entry.label(),
                });
            }
        }
        after = page.records().last().map(|entry| entry.ordinal());
        if !page.has_more() {
            return Ok(());
        }
        if after.is_none() {
            return Err(InputAdmissionBuildError::AssetReferencePageStalled);
        }
    }
}
