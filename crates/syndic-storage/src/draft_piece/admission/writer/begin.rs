use std::num::NonZeroU64;

use beryl_home_store::{DomainReader, MutationBuilder};

use crate::SyndicMutationError;
use crate::codec::{DraftImageLabelProtectionHeadsFamily, ImageLabelAuthorityHeadsFamily};
use crate::domain::SyndicDomain;
use crate::mutation::{point, required};

use super::super::*;
use super::model::*;
pub(crate) struct PreparedDraftMarkerWriterBeginV1 {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    readiness_receipt: DraftMarkerAdmissionReceiptKeyV1,
}

pub(crate) fn prepare_draft_marker_writer_begin_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
) -> Result<PreparedDraftMarkerWriterBeginV1, SyndicMutationError> {
    let binding = admission.binding();
    let owner = binding.owner();
    let head = required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    let capacity =
        required::<DraftMarkerAdmissionCapacityFamily>(reader, &DraftMarkerAdmissionCapacityKeyV1)?;
    let selected = head
        .selected_receipt()
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let readiness_receipt = DraftMarkerAdmissionReceiptKeyV1::new(owner, selected);
    let receipt = required::<DraftMarkerAdmissionReceiptsFamily>(reader, &readiness_receipt)?;
    let authority =
        required::<ImageLabelAuthorityHeadsFamily>(reader, &binding.label_authority().thread_id())?;
    let protection = required::<DraftImageLabelProtectionHeadsFamily>(
        reader,
        &binding.protection().thread_id(),
    )?;
    if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Ready
        || head.home_generation() != binding.home_generation()
        || head.owner() != owner
        || head.occurrence_commitment() != binding.occurrence_commitment()
        || head.target_root() != binding.sealed_target_root()
        || head.target_root() != admission.target_root()
        || head.remaining_builder_count() != admission.remaining_count()
        || receipt.owner() != owner
        || receipt.command_id() != selected
        || receipt.transition() != DraftMarkerAdmissionReceiptTransitionV1::Assignment
        || receipt.request_commitment() != head.request_commitment()
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
        || authority != binding.label_authority()
        || protection != binding.protection()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let next_revision = NonZeroU64::new(
        head.revision()
            .get()
            .checked_add(1)
            .ok_or(SyndicMutationError::IdentityCollision)?,
    )
    .ok_or(SyndicMutationError::IdentityCollision)?;
    let provisional = DraftMarkerAdmissionHeadV1::new(
        owner,
        next_revision,
        head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Staging,
        head.request_commitment(),
        head.custody_commitment(),
        head.next_page_ordinal(),
        head.ingestion_association_cursor(),
        head.evidence_eof(),
        None,
        head.source_root(),
        head.target_root(),
        head.occurrence_commitment(),
        head.unassigned_count(),
        head.assignment_continuation(),
        head.remaining_builder_count(),
        DraftMarkerAdmissionRetainedChargeV1::new(1, head.target_root().count(), 0),
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let prior_metadata = encoded_head_record_charge(&owner, &head)
        .and_then(|value| {
            value
                .checked_add(encoded_receipt_record_charge(&readiness_receipt, &receipt)?)
                .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
        })
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_metadata = encoded_head_record_charge(&owner, &provisional)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_charge = head
        .charge()
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
        head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Staging,
        head.request_commitment(),
        head.custody_commitment(),
        head.next_page_ordinal(),
        head.ingestion_association_cursor(),
        head.evidence_eof(),
        None,
        head.source_root(),
        head.target_root(),
        head.occurrence_commitment(),
        head.unassigned_count(),
        head.assignment_continuation(),
        head.remaining_builder_count(),
        successor_charge,
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let aggregate = capacity
        .charge()
        .checked_sub(required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?.charge())
        .and_then(|charge| charge.checked_add(successor_charge))
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let capacity = DraftMarkerAdmissionCapacityV1::new(
        NonZeroU64::new(
            capacity
                .revision()
                .get()
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
        )
        .ok_or(SyndicMutationError::IdentityCollision)?,
        aggregate,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    Ok(PreparedDraftMarkerWriterBeginV1 {
        capacity,
        head,
        readiness_receipt,
    })
}

pub(crate) fn contribute_draft_marker_writer_begin_v1(
    prepared: PreparedDraftMarkerWriterBeginV1,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    mutations.put::<DraftMarkerAdmissionCapacityCodec>(
        &DraftMarkerAdmissionCapacityKeyV1,
        &prepared.capacity,
    )?;
    mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
    mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&prepared.readiness_receipt)?;
    Ok(())
}

pub(crate) fn prepare_draft_marker_writer_building_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
) -> Result<DraftMarkerAdmissionHeadV1, SyndicMutationError> {
    let binding = admission.binding();
    let owner = binding.owner();
    let head = required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Staging
        || head.home_generation() != binding.home_generation()
        || head.owner() != owner
        || head.occurrence_commitment() != binding.occurrence_commitment()
        || head.target_root() != admission.target_root()
        || head.remaining_builder_count() != admission.remaining_count()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    DraftMarkerAdmissionHeadV1::new(
        owner,
        NonZeroU64::new(
            head.revision()
                .get()
                .checked_add(1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
        )
        .ok_or(SyndicMutationError::IdentityCollision)?,
        head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Building,
        head.request_commitment(),
        head.custody_commitment(),
        head.next_page_ordinal(),
        head.ingestion_association_cursor(),
        head.evidence_eof(),
        None,
        head.source_root(),
        head.target_root(),
        head.occurrence_commitment(),
        head.unassigned_count(),
        None,
        head.remaining_builder_count(),
        head.charge(),
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)
}

pub(crate) fn draft_marker_writer_head_is_exact_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    lifecycle: DraftMarkerAdmissionLifecycleV1,
) -> Result<bool, SyndicMutationError> {
    let binding = admission.binding();
    let owner = binding.owner();
    let Some(head) = point::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)? else {
        return Ok(false);
    };
    Ok(head.lifecycle() == lifecycle
        && matches!(
            lifecycle,
            DraftMarkerAdmissionLifecycleV1::Staging | DraftMarkerAdmissionLifecycleV1::Building
        )
        && head.home_generation() == binding.home_generation()
        && head.owner() == owner
        && head.occurrence_commitment() == binding.occurrence_commitment()
        && head.target_root() == admission.target_root()
        && head.remaining_builder_count() == admission.remaining_count())
}
