use beryl_home_store::{CommandBuildError, HomeCommand, HomeStore, ReadError};
use beryl_model::AssetId;
use beryl_state::{
    AddAssetReferences, AssetReferenceAddition, AssetReferenceBatchError, AssetReferenceMove,
    AssetReferenceOwner, AssetState, AssetValueError, MoveAssetReferences, UnixMillis,
};
use syndic_storage::{
    AcceptedInputAdmission, CancelReplacementEdit, IdleSubmission, StartReplacementEdit,
    SyndicStorage,
};

/// Failure while composing one exact cross-domain input command.
#[derive(Debug, thiserror::Error)]
pub enum InputAdmissionBuildError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Command(#[from] CommandBuildError),
    #[error(transparent)]
    AssetValue(#[from] AssetValueError),
    #[error(transparent)]
    AssetBatch(#[from] AssetReferenceBatchError),
    #[error("replacement target references missing asset metadata {0:?}")]
    MissingAsset(AssetId),
    #[error("replacement target has no durable historical asset reference {0:?}")]
    MissingHistoricalReference(AssetReferenceOwner),
    #[error("replacement target historical asset reference disagrees with its Syndic marker")]
    HistoricalReferenceMismatch,
}

/// Builds one atomic idle-submission command, including every marker-owner move.
pub fn idle_submission_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    submission: IdleSubmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let moves = submission
        .markers()
        .markers()
        .iter()
        .map(|marker| {
            AssetReferenceMove::new(
                AssetReferenceOwner::CurrentDraftMarker {
                    draft_id: submission.draft_id(),
                    marker_id: marker.marker_id(),
                },
                AssetReferenceOwner::SubmittedTurnItemMarker {
                    item_id: submission.user_item_id(),
                    marker_id: marker.marker_id(),
                },
                marker.asset_id(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    command_with_moves(
        store,
        assets,
        syndic.submit_idle_draft(syndic.revision(store)?, submission),
        moves,
    )
}

/// Builds one atomic accepted-input command, including every marker-owner move.
pub fn accepted_input_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    admission: AcceptedInputAdmission,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let moves = admission
        .markers()
        .markers()
        .iter()
        .map(|marker| {
            AssetReferenceMove::new(
                AssetReferenceOwner::CurrentDraftMarker {
                    draft_id: admission.draft_id(),
                    marker_id: marker.marker_id(),
                },
                AssetReferenceOwner::AcceptedInputMarker {
                    input_id: admission.accepted_input_id(),
                    marker_id: marker.marker_id(),
                },
                marker.asset_id(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    command_with_moves(
        store,
        assets,
        syndic.admit_accepted_input(syndic.revision(store)?, admission),
        moves,
    )
}

/// Builds one atomic replacement-start command and duplicates target asset ownership.
pub fn start_replacement_edit_command(
    store: &HomeStore,
    syndic: SyndicStorage,
    assets: AssetState,
    edit: StartReplacementEdit,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let created_at = UnixMillis::new(edit.started_at().unix_millis());
    let mut additions = Vec::with_capacity(edit.markers().markers().len());
    for marker in edit.markers().markers() {
        let historical_owner = AssetReferenceOwner::SubmittedTurnItemMarker {
            item_id: edit.target_item_id(),
            marker_id: marker.marker_id(),
        };
        let historical = assets.reference(store, historical_owner)?.ok_or(
            InputAdmissionBuildError::MissingHistoricalReference(historical_owner),
        )?;
        if historical.asset_id() != marker.asset_id() {
            return Err(InputAdmissionBuildError::HistoricalReferenceMismatch);
        }
        let metadata = assets
            .metadata(store, marker.asset_id())?
            .ok_or(InputAdmissionBuildError::MissingAsset(marker.asset_id()))?;
        additions.push(AssetReferenceAddition::new(
            AssetReferenceOwner::CurrentDraftMarker {
                draft_id: edit.draft_id(),
                marker_id: marker.marker_id(),
            },
            marker.asset_id(),
            metadata.revision(),
            created_at,
        )?);
    }

    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(syndic.start_replacement_edit(syndic.revision(store)?, edit))?;
    if !additions.is_empty() {
        let addition = AddAssetReferences::new(additions)?;
        command.add(assets.add_references(assets.revision(store)?, addition))?;
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

fn command_with_moves(
    store: &HomeStore,
    assets: AssetState,
    syndic_contribution: beryl_home_store::MutationContribution,
    moves: Vec<AssetReferenceMove>,
) -> Result<HomeCommand, InputAdmissionBuildError> {
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(syndic_contribution)?;
    if !moves.is_empty() {
        let movement = MoveAssetReferences::new(moves)?;
        command.add(assets.move_references(assets.revision(store)?, movement))?;
    }
    Ok(command)
}
