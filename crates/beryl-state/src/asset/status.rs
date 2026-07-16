use std::collections::BTreeSet;

use beryl_home_store::{HomeStore, ReadError};
use beryl_model::AssetId;

use super::{
    AddAssetReferences, AssetDomain, AssetReferenceAddition, AssetReferenceOwner,
    AssetReferenceRecord, AssetReferenceStatusError, AssetState, MoveAssetReferences,
    codec::{AssetReferenceIndexCodec, AssetReferenceIndexKey},
    mutation::add_references::grouped_assets,
    reference_point_limit,
};

/// Coherent exact durable state of one complete reference-move description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferenceMoveStatus {
    Source,
    Target,
    CollisionOrMixed,
}

/// Coherent exact durable state of one complete reference-addition description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetReferenceAdditionStatus {
    Absent,
    Target,
    CollisionOrMixed,
}

pub(super) fn move_status(
    state: &AssetState,
    store: &HomeStore,
    command: &MoveAssetReferences,
) -> Result<AssetReferenceMoveStatus, AssetReferenceStatusError> {
    let before = state.revision(store)?;
    let mut all_source = true;
    let mut all_target = true;
    let mut checked_assets = BTreeSet::new();

    for reference_move in command.moves() {
        if checked_assets.insert(reference_move.asset_id())
            && state.metadata(store, reference_move.asset_id())?.is_none()
        {
            all_source = false;
            all_target = false;
        }
        let source = reference_observation(
            state,
            store,
            reference_move.source(),
            reference_move.asset_id(),
        )?
        .exact_state(reference_move.source(), reference_move.asset_id());
        let target = reference_observation(
            state,
            store,
            reference_move.destination(),
            reference_move.asset_id(),
        )?
        .exact_state(reference_move.destination(), reference_move.asset_id());
        all_source &=
            source == ExactReferenceState::Present && target == ExactReferenceState::Absent;
        all_target &=
            source == ExactReferenceState::Absent && target == ExactReferenceState::Present;
    }

    confirm_unchanged(state, store, before)?;
    Ok(if all_source {
        AssetReferenceMoveStatus::Source
    } else if all_target {
        AssetReferenceMoveStatus::Target
    } else {
        AssetReferenceMoveStatus::CollisionOrMixed
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactReferenceState {
    Absent,
    Present,
    Collision,
}

struct ReferenceObservation {
    primary: Option<AssetReferenceRecord>,
    indexed: Option<AssetReferenceRecord>,
}

impl ReferenceObservation {
    fn exact_state(&self, owner: AssetReferenceOwner, asset_id: AssetId) -> ExactReferenceState {
        match (&self.primary, &self.indexed) {
            (None, None) => ExactReferenceState::Absent,
            (Some(primary), Some(indexed))
                if primary.owner() == owner
                    && primary.asset_id() == asset_id
                    && primary == indexed =>
            {
                ExactReferenceState::Present
            }
            _ => ExactReferenceState::Collision,
        }
    }
}

fn reference_observation(
    state: &AssetState,
    store: &HomeStore,
    owner: AssetReferenceOwner,
    asset_id: AssetId,
) -> Result<ReferenceObservation, ReadError> {
    let primary = state.reference(store, owner)?;
    let indexed = store.read_point::<AssetDomain, AssetReferenceIndexCodec>(
        state.handle,
        &AssetReferenceIndexKey { asset_id, owner },
        reference_point_limit(),
    )?;
    Ok(ReferenceObservation { primary, indexed })
}

fn exact_addition_record(record: &AssetReferenceRecord, addition: &AssetReferenceAddition) -> bool {
    record.owner() == addition.owner()
        && record.asset_id() == addition.asset_id()
        && record.created_at() == addition.created_at()
}

fn confirm_unchanged(
    state: &AssetState,
    store: &HomeStore,
    before: beryl_model::DomainRevision,
) -> Result<(), AssetReferenceStatusError> {
    if state.revision(store)? == before {
        Ok(())
    } else {
        Err(AssetReferenceStatusError::ConcurrentChange)
    }
}

pub(super) fn addition_status(
    state: &AssetState,
    store: &HomeStore,
    command: &AddAssetReferences,
) -> Result<AssetReferenceAdditionStatus, AssetReferenceStatusError> {
    let before = state.revision(store)?;
    let mut all_absent = true;
    let mut all_target = true;

    for (asset_id, (expected_revision, _)) in grouped_assets(command.additions()) {
        let metadata = state.metadata(store, asset_id)?;
        all_absent &= metadata
            .as_ref()
            .is_some_and(|record| record.revision() == expected_revision);
        let target_revision = expected_revision
            .checked_next()
            .expect("addition batches reject exhausted record revisions");
        all_target &= metadata
            .as_ref()
            .is_some_and(|record| record.revision() == target_revision);
    }

    for addition in command.additions() {
        let observation =
            reference_observation(state, store, addition.owner(), addition.asset_id())?;
        let exact_state = observation.exact_state(addition.owner(), addition.asset_id());
        all_absent &= exact_state == ExactReferenceState::Absent;
        all_target &= exact_state == ExactReferenceState::Present
            && observation
                .primary
                .as_ref()
                .is_some_and(|record| exact_addition_record(record, addition));
    }

    confirm_unchanged(state, store, before)?;
    Ok(if all_absent {
        AssetReferenceAdditionStatus::Absent
    } else if all_target {
        AssetReferenceAdditionStatus::Target
    } else {
        AssetReferenceAdditionStatus::CollisionOrMixed
    })
}
