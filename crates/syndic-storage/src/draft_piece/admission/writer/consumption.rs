use std::num::NonZeroU64;

use beryl_home_store::{DomainReader, MutationBuilder};
use sha2::{Digest, Sha256};

use crate::SyndicMutationError;
use crate::domain::SyndicDomain;
use crate::draft_piece::DraftPieceMarkerV1;
use crate::mutation::required;

use super::super::index::{
    PreparedDraftMarkerAdmissionConsumptionV1, prepare_draft_marker_admission_consumption_v1,
};
use super::super::*;
use super::model::*;
#[derive(Clone)]
pub(crate) struct PreparedDraftMarkerWriterConsumptionV1 {
    admission: DraftMarkerWriterAdmissionV1,
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    index: PreparedDraftMarkerAdmissionConsumptionV1,
}

impl PreparedDraftMarkerWriterConsumptionV1 {
    pub(crate) const fn admission(&self) -> DraftMarkerWriterAdmissionV1 {
        self.admission
    }
    pub(crate) const fn index(&self) -> &PreparedDraftMarkerAdmissionConsumptionV1 {
        &self.index
    }
}

pub(crate) fn draft_marker_writer_consumption_identity_v1(
    owner: DraftMarkerAdmissionOwnerV1,
    transition_ordinal: u64,
) -> Result<DraftMarkerAdmissionPageIdentityV1, SyndicMutationError> {
    let mut hasher = Sha256::new();
    hasher.update(b"syndic/draft-marker-writer-consumption-command/v1");
    hasher.update(owner.operation_id().as_bytes());
    hasher.update(transition_ordinal.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut command = [0; 16];
    command.copy_from_slice(&digest[..16]);
    Ok(DraftMarkerAdmissionPageIdentityV1::new(
        DraftMarkerAdmissionCommandIdV1::from_bytes(command),
        NonZeroU64::new(transition_ordinal).ok_or(SyndicMutationError::IdentityCollision)?,
    ))
}

pub(crate) fn prepare_draft_marker_writer_consumption_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    marker: DraftPieceMarkerV1,
    transition_ordinal: u64,
) -> Result<PreparedDraftMarkerWriterConsumptionV1, SyndicMutationError> {
    let owner = admission.binding().owner();
    let prior_head = required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    let prior_capacity =
        required::<DraftMarkerAdmissionCapacityFamily>(reader, &DraftMarkerAdmissionCapacityKeyV1)?;
    if prior_head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Building
        || prior_head.target_root() != admission.target_root()
        || prior_head.remaining_builder_count() != admission.remaining_count()
        || prior_head.occurrence_commitment() != admission.binding().occurrence_commitment()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let page = draft_marker_writer_consumption_identity_v1(owner, transition_ordinal)?;
    let index = prepare_draft_marker_admission_consumption_v1(
        reader,
        owner,
        admission.target_root(),
        marker,
        page,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_admission = admission
        .with_target_root(index.target_root())
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let delta = index.retained_charge_delta();
    let provisional_charge = prior_head
        .charge()
        .checked_add(delta.added())
        .and_then(|charge| charge.checked_sub(delta.removed()))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let next_revision = NonZeroU64::new(
        prior_head
            .revision()
            .get()
            .checked_add(1)
            .ok_or(SyndicMutationError::IdentityCollision)?,
    )
    .ok_or(SyndicMutationError::IdentityCollision)?;
    let provisional = DraftMarkerAdmissionHeadV1::new(
        owner,
        next_revision,
        prior_head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Building,
        prior_head.request_commitment(),
        prior_head.custody_commitment(),
        prior_head.next_page_ordinal(),
        0,
        true,
        None,
        prior_head.source_root(),
        successor_admission.target_root(),
        prior_head.occurrence_commitment(),
        0,
        None,
        successor_admission.remaining_count(),
        provisional_charge,
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let prior_metadata = encoded_head_record_charge(&owner, &prior_head)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_metadata = encoded_head_record_charge(&owner, &provisional)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_charge = provisional_charge
        .checked_sub(DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            0,
            prior_metadata,
        ))
        .and_then(|charge| {
            charge.checked_add(DraftMarkerAdmissionRetainedChargeV1::new(
                0,
                0,
                successor_metadata,
            ))
        })
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let head = DraftMarkerAdmissionHeadV1::new(
        owner,
        next_revision,
        prior_head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Building,
        prior_head.request_commitment(),
        prior_head.custody_commitment(),
        prior_head.next_page_ordinal(),
        0,
        true,
        None,
        prior_head.source_root(),
        successor_admission.target_root(),
        prior_head.occurrence_commitment(),
        0,
        None,
        successor_admission.remaining_count(),
        successor_charge,
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let aggregate = prior_capacity
        .charge()
        .checked_sub(prior_head.charge())
        .and_then(|charge| charge.checked_add(successor_charge))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let capacity = DraftMarkerAdmissionCapacityV1::new(
        NonZeroU64::new(
            prior_capacity
                .revision()
                .get()
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
        )
        .ok_or(SyndicMutationError::IdentityCollision)?,
        aggregate,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    Ok(PreparedDraftMarkerWriterConsumptionV1 {
        admission: successor_admission,
        capacity,
        head,
        index,
    })
}

pub(crate) fn contribute_draft_marker_writer_consumption_v1(
    prepared: PreparedDraftMarkerWriterConsumptionV1,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    for node in prepared.index.puts() {
        mutations.put::<DraftMarkerAdmissionNodesCodec>(&node.key(), node)?;
    }
    for node in prepared.index.deletions() {
        mutations.delete::<DraftMarkerAdmissionNodesCodec>(&node.key())?;
    }
    mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
    mutations.put::<DraftMarkerAdmissionCapacityCodec>(
        &DraftMarkerAdmissionCapacityKeyV1,
        &prepared.capacity,
    )?;
    Ok(())
}
