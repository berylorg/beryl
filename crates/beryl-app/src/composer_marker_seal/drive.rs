use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    OrderedMarkerAssetSummaryV1, SequentialMarkerSummaryV1, ordered_marker_asset_digest_seed,
    sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetReferencePageEntry, AssetReferenceSetCompletion,
    AssetReferenceSetStagingAuthority, AssetState, BeginAssetReferenceSet, SealAssetReferenceSet,
};
use syndic_storage::{DraftMarkerSealProofV1, DraftMarkerSealStatusV1, SyndicStorage};

use super::{
    CommandFault, DraftMarkerSealCommandStage, DraftMarkerSealDriveOutcome,
    DraftMarkerSealFlightRequest, DraftMarkerSealServiceError, DriveUpdate, DurableCommandResult,
    FlightPhase, ReconcileFault,
    durability::{execute_command, require_matching_frontier, settle_command},
};

pub(super) fn drive_begin(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    request: DraftMarkerSealFlightRequest,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
) -> Result<DriveUpdate, DraftMarkerSealServiceError> {
    let seal_request = request.seal_request();
    let begin_asset = BeginAssetReferenceSet::new(request.staging_authority());
    let staging = (!request.is_empty()).then_some(request.staging_authority());
    match storage.draft_marker_seal_status(store, seal_request.key())? {
        DraftMarkerSealStatusV1::Absent => {
            let prepared = storage.prepare_draft_marker_seal_begin(store, seal_request)?;
            let mut command = HomeCommand::new(store.home_revision()?);
            command.add(storage.begin_draft_marker_seal(storage.revision(store)?, prepared))?;
            if !request.is_empty() {
                command.add(assets.begin_reference_set(assets.revision(store)?, begin_asset))?;
            }
            let outcome = execute_command(store, command, command_fault);
            match settle_command(store, outcome, storage, request, reconcile_fault)? {
                DurableCommandResult::ExactOld => Ok(DriveUpdate::Keep(
                    FlightPhase::PendingBegin,
                    DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::Begin),
                )),
                DurableCommandResult::ExactNew => Ok(DriveUpdate::Keep(
                    FlightPhase::Streaming { staging },
                    DraftMarkerSealDriveOutcome::Progress,
                )),
            }
        }
        DraftMarkerSealStatusV1::Open {
            completed_marker_count,
        } => {
            if let Some(staging) = staging {
                let manifest = require_building_completion(store, assets, staging)?;
                if manifest.entry_frontier() != completed_marker_count {
                    return Err(DraftMarkerSealServiceError::FrontierMismatch);
                }
            } else if completed_marker_count != 0 {
                return Err(DraftMarkerSealServiceError::FrontierMismatch);
            }
            Ok(DriveUpdate::Keep(
                FlightPhase::Streaming { staging },
                DraftMarkerSealDriveOutcome::Progress,
            ))
        }
        DraftMarkerSealStatusV1::Sealed(syndic, _) if request.is_empty() => Ok(
            DriveUpdate::Complete(DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic }),
        ),
        DraftMarkerSealStatusV1::Sealed(syndic, _) => {
            let staging = staging.expect("nonempty marker seal has Asset staging authority");
            match assets.complete_reference_set(
                store,
                staging,
                syndic.sequential(),
                syndic.ordered_assets(),
            )? {
                AssetReferenceSetCompletion::Building(manifest) => {
                    require_matching_frontier(&manifest, syndic)?;
                    Ok(DriveUpdate::Keep(
                        FlightPhase::SealingAsset { staging, syndic },
                        DraftMarkerSealDriveOutcome::Progress,
                    ))
                }
                AssetReferenceSetCompletion::Sealed(assets) => Ok(DriveUpdate::Complete(
                    DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets },
                )),
            }
        }
        DraftMarkerSealStatusV1::Cancelled(_)
        | DraftMarkerSealStatusV1::Failed { .. }
        | DraftMarkerSealStatusV1::Superseded { .. } => {
            Err(DraftMarkerSealServiceError::DurableTerminal)
        }
    }
}

pub(super) fn drive_page(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    request: DraftMarkerSealFlightRequest,
    staging: Option<AssetReferenceSetStagingAuthority>,
    page_limit: usize,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
) -> Result<DriveUpdate, DraftMarkerSealServiceError> {
    let seal_request = request.seal_request();
    let Some(prepared) = storage.prepare_draft_marker_seal_advance_with_limit(
        store,
        seal_request.key(),
        page_limit,
    )?
    else {
        return completed_or_terminal(store, storage, assets, request, staging);
    };
    let release = prepared.page().release();
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(storage.advance_draft_marker_seal(storage.revision(store)?, &prepared))?;
    if let Some(staging) = staging {
        let manifest = require_building_completion(store, assets, staging)?;
        if manifest.entry_frontier() != release.source_frontier()
            || prepared.page().markers().is_empty()
        {
            return Err(DraftMarkerSealServiceError::FrontierMismatch);
        }
        let entries = prepared
            .page()
            .markers()
            .iter()
            .map(|marker| {
                AssetReferencePageEntry::new(marker.marker_id(), marker.label(), marker.asset_id())
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        command.add(assets.append_reference_page(
            assets.revision(store)?,
            AppendAssetReferencePage::new(manifest.build_proof(), entries)?,
        ))?;
    } else if !prepared.page().markers().is_empty() {
        return Err(DraftMarkerSealServiceError::FrontierMismatch);
    }
    let outcome = execute_command(store, command, command_fault);
    match settle_command(store, outcome, storage, request, reconcile_fault)? {
        DurableCommandResult::ExactOld => Ok(DriveUpdate::Keep(
            FlightPhase::Streaming { staging },
            DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::Page),
        )),
        DurableCommandResult::ExactNew => {
            let status = storage.draft_marker_seal_status(store, seal_request.key())?;
            if let Some(staging) = staging {
                let manifest = require_building_completion(store, assets, staging)?;
                if manifest.entry_frontier() != release.target_frontier() {
                    return Err(DraftMarkerSealServiceError::FrontierMismatch);
                }
            }
            match status {
                DraftMarkerSealStatusV1::Open {
                    completed_marker_count,
                } if completed_marker_count == release.target_frontier()
                    && !prepared.page().exact_eof() =>
                {
                    Ok(DriveUpdate::Keep(
                        FlightPhase::Streaming { staging },
                        DraftMarkerSealDriveOutcome::Progress,
                    ))
                }
                DraftMarkerSealStatusV1::Sealed(syndic, _)
                    if prepared.page().exact_eof() && request.is_empty() =>
                {
                    Ok(DriveUpdate::Complete(
                        DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic },
                    ))
                }
                DraftMarkerSealStatusV1::Sealed(syndic, _) if prepared.page().exact_eof() => {
                    let staging = staging.expect("nonempty marker page has Asset staging");
                    completed_nonempty(store, assets, staging, syndic)
                }
                _ => Err(DraftMarkerSealServiceError::FrontierMismatch),
            }
        }
    }
}

fn completed_or_terminal(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    request: DraftMarkerSealFlightRequest,
    staging: Option<AssetReferenceSetStagingAuthority>,
) -> Result<DriveUpdate, DraftMarkerSealServiceError> {
    match storage.draft_marker_seal_status(store, request.seal_request().key())? {
        DraftMarkerSealStatusV1::Sealed(syndic, _) if request.is_empty() => Ok(
            DriveUpdate::Complete(DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic }),
        ),
        DraftMarkerSealStatusV1::Sealed(syndic, _) => {
            let staging = staging.ok_or(DraftMarkerSealServiceError::FrontierMismatch)?;
            completed_nonempty(store, assets, staging, syndic)
        }
        DraftMarkerSealStatusV1::Cancelled(_)
        | DraftMarkerSealStatusV1::Failed { .. }
        | DraftMarkerSealStatusV1::Superseded { .. } => {
            Err(DraftMarkerSealServiceError::DurableTerminal)
        }
        DraftMarkerSealStatusV1::Absent | DraftMarkerSealStatusV1::Open { .. } => {
            Err(DraftMarkerSealServiceError::FrontierMismatch)
        }
    }
}

pub(super) fn drive_asset_seal(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    request: DraftMarkerSealFlightRequest,
    staging: AssetReferenceSetStagingAuthority,
    syndic: DraftMarkerSealProofV1,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
) -> Result<DriveUpdate, DraftMarkerSealServiceError> {
    let manifest = match assets.complete_reference_set(
        store,
        staging,
        syndic.sequential(),
        syndic.ordered_assets(),
    )? {
        AssetReferenceSetCompletion::Building(manifest) => manifest,
        AssetReferenceSetCompletion::Sealed(assets) => {
            return Ok(DriveUpdate::Complete(
                DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets },
            ));
        }
    };
    require_matching_frontier(&manifest, syndic)?;
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        syndic.sequential(),
        syndic.ordered_assets(),
    )?;
    let proof = seal.sealed_proof();
    let mut command = HomeCommand::new(store.home_revision()?);
    command.add(assets.seal_reference_set(assets.revision(store)?, seal))?;
    let outcome = execute_command(store, command, command_fault);
    match settle_command(store, outcome, storage, request, reconcile_fault)? {
        DurableCommandResult::ExactOld => Ok(DriveUpdate::Keep(
            FlightPhase::SealingAsset { staging, syndic },
            DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::AssetSeal),
        )),
        DurableCommandResult::ExactNew => match assets.complete_reference_set(
            store,
            staging,
            syndic.sequential(),
            syndic.ordered_assets(),
        )? {
            AssetReferenceSetCompletion::Building(_) => {
                Err(DraftMarkerSealServiceError::FrontierMismatch)
            }
            AssetReferenceSetCompletion::Sealed(actual) if actual == proof => Ok(
                DriveUpdate::Complete(DraftMarkerSealDriveOutcome::ChangedNonempty {
                    syndic,
                    assets: actual,
                }),
            ),
            AssetReferenceSetCompletion::Sealed(_) => {
                Err(DraftMarkerSealServiceError::FrontierMismatch)
            }
        },
    }
}

fn require_building_completion(
    store: &HomeStore,
    assets: AssetState,
    staging: AssetReferenceSetStagingAuthority,
) -> Result<beryl_state::AssetReferenceSetManifest, DraftMarkerSealServiceError> {
    match assets.complete_reference_set(
        store,
        staging,
        empty_sequential(),
        empty_ordered_assets(),
    )? {
        AssetReferenceSetCompletion::Building(manifest) => Ok(manifest),
        AssetReferenceSetCompletion::Sealed(_) => {
            Err(DraftMarkerSealServiceError::FrontierMismatch)
        }
    }
}

fn completed_nonempty(
    store: &HomeStore,
    assets: AssetState,
    staging: AssetReferenceSetStagingAuthority,
    syndic: DraftMarkerSealProofV1,
) -> Result<DriveUpdate, DraftMarkerSealServiceError> {
    match assets.complete_reference_set(
        store,
        staging,
        syndic.sequential(),
        syndic.ordered_assets(),
    )? {
        AssetReferenceSetCompletion::Building(manifest) => {
            require_matching_frontier(&manifest, syndic)?;
            Ok(DriveUpdate::Keep(
                FlightPhase::SealingAsset { staging, syndic },
                DraftMarkerSealDriveOutcome::Progress,
            ))
        }
        AssetReferenceSetCompletion::Sealed(assets) => Ok(DriveUpdate::Complete(
            DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets },
        )),
    }
}

fn empty_sequential() -> SequentialMarkerSummaryV1 {
    SequentialMarkerSummaryV1::new(sequential_marker_digest_seed(), 0, None)
        .expect("the canonical empty sequential marker summary is valid")
}

fn empty_ordered_assets() -> OrderedMarkerAssetSummaryV1 {
    OrderedMarkerAssetSummaryV1::new(ordered_marker_asset_digest_seed(), 0)
}
