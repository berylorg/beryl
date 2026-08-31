use std::num::NonZeroU64;

use sha2::{Digest, Sha256};

use crate::codec::{
    DraftImageLabelProtectionHeadsCodec, DraftImageLabelProtectionHeadsFamily,
    ImageLabelAuthorityHeadsFamily,
};
use crate::domain::SyndicDomain;
use crate::draft_piece::DraftPieceDigestV1;
use crate::mutation::{point, required};
use crate::{DraftImageLabelProtectionHeadV1, SyndicMutationError};
use beryl_home_store::{DomainReader, MutationBuilder};

use super::super::*;
use super::model::*;
use crate::draft_piece::admission::terminal::closure::{
    terminal_receipt_is_exact, terminal_source_closure, terminal_target_closure,
};

const STAGING_TERMINAL_COMMAND_DOMAIN: &[u8] =
    b"syndic/draft-marker-writer-staging-terminal-command/v1";
pub(crate) struct PreparedDraftMarkerWriterSettlementV1 {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    protection: Option<DraftImageLabelProtectionHeadV1>,
}

pub(crate) fn prepare_draft_marker_writer_settlement_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    publish_protection: bool,
) -> Result<PreparedDraftMarkerWriterSettlementV1, SyndicMutationError> {
    if !admission.is_empty() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let binding = admission.binding();
    let owner = binding.owner();
    let prior_head = required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    let prior_capacity =
        required::<DraftMarkerAdmissionCapacityFamily>(reader, &DraftMarkerAdmissionCapacityKeyV1)?;
    let authority =
        required::<ImageLabelAuthorityHeadsFamily>(reader, &binding.label_authority().thread_id())?;
    let protection = required::<DraftImageLabelProtectionHeadsFamily>(
        reader,
        &binding.protection().thread_id(),
    )?;
    if authority != binding.label_authority()
        || !protection.is_exact()
        || protection.thread_id() != binding.protection().thread_id()
        || protection.revision() < binding.protection().revision()
        || protection.protected_maximum() < binding.protection().protected_maximum()
        || (protection.revision() == binding.protection().revision()
            && protection != binding.protection())
        || prior_head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Building
        || prior_head.home_generation() != binding.home_generation()
        || prior_head.target_root() != admission.target_root()
        || prior_head.remaining_builder_count() != 0
        || prior_head.occurrence_commitment() != binding.occurrence_commitment()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
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
        DraftMarkerAdmissionLifecycleV1::Settled,
        prior_head.request_commitment(),
        prior_head.custody_commitment(),
        prior_head.next_page_ordinal(),
        0,
        true,
        None,
        prior_head.source_root(),
        admission.target_root(),
        prior_head.occurrence_commitment(),
        0,
        None,
        0,
        DraftMarkerAdmissionRetainedChargeV1::new(1, 0, 0),
        None,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let prior_metadata = encoded_head_record_charge(&owner, &prior_head)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_metadata = encoded_head_record_charge(&owner, &provisional)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_charge = prior_head
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
        prior_head.home_generation(),
        DraftMarkerAdmissionLifecycleV1::Settled,
        prior_head.request_commitment(),
        prior_head.custody_commitment(),
        prior_head.next_page_ordinal(),
        0,
        true,
        None,
        prior_head.source_root(),
        admission.target_root(),
        prior_head.occurrence_commitment(),
        0,
        None,
        0,
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
    let protection = publish_protection
        .then(|| binding.allocation_range())
        .flatten()
        .filter(|range| !protection.protected_maximum().contains(range.last()))
        .map(|range| {
            protection
                .advanced(crate::ImageLabelFrontier::from_raw(range.last().get()))
                .map_err(|_| SyndicMutationError::IdentityCollision)
        })
        .transpose()?;
    Ok(PreparedDraftMarkerWriterSettlementV1 {
        capacity,
        head,
        protection,
    })
}

pub(crate) fn contribute_draft_marker_writer_settlement_v1(
    prepared: PreparedDraftMarkerWriterSettlementV1,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
    mutations.put::<DraftMarkerAdmissionCapacityCodec>(
        &DraftMarkerAdmissionCapacityKeyV1,
        &prepared.capacity,
    )?;
    if let Some(protection) = prepared.protection {
        mutations
            .put::<DraftImageLabelProtectionHeadsCodec>(&protection.thread_id(), &protection)?;
    }
    Ok(())
}

pub(crate) struct PreparedDraftMarkerWriterTerminalV1 {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt: DraftMarkerAdmissionReplayReceiptV1,
}

pub(crate) fn prepare_draft_marker_writer_terminal_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    staging_terminal_digest: DraftPieceDigestV1,
) -> Result<PreparedDraftMarkerWriterTerminalV1, SyndicMutationError> {
    let owner = admission.binding().owner();
    let prior_head = required::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    let prior_capacity =
        required::<DraftMarkerAdmissionCapacityFamily>(reader, &DraftMarkerAdmissionCapacityKeyV1)?;
    if !matches!(
        prior_head.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::Staging | DraftMarkerAdmissionLifecycleV1::Building
    ) || prior_head.home_generation() != admission.binding().home_generation()
        || prior_head.occurrence_commitment() != admission.binding().occurrence_commitment()
        || prior_head.target_root() != admission.target_root()
        || prior_head.remaining_builder_count() != admission.remaining_count()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    let command = staging_terminal_command(owner, staging_terminal_digest);
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        owner,
        command,
        prior_head.next_page_ordinal(),
        prior_head.request_commitment(),
        terminal_source_closure(owner, command),
        terminal_target_closure(&prior_head),
        prior_head.source_root(),
        empty_source,
        prior_head.target_root(),
        empty_target,
        Box::<[DraftMarkerAdmissionChildV1]>::default(),
        DraftMarkerAdmissionReceiptTransitionV1::TerminalCleanup,
    )
    .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(owner, command);
    if point::<DraftMarkerAdmissionReceiptsFamily>(reader, &receipt_key)?.is_some() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let next_revision = NonZeroU64::new(
        prior_head
            .revision()
            .get()
            .checked_add(1)
            .ok_or(SyndicMutationError::IdentityCollision)?,
    )
    .ok_or(SyndicMutationError::IdentityCollision)?;
    let provisional_charge =
        DraftMarkerAdmissionRetainedChargeV1::new(1, prior_head.charge().associations(), 0);
    let make_head = |charge| {
        DraftMarkerAdmissionHeadV1::new(
            owner,
            next_revision,
            prior_head.home_generation(),
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup,
            prior_head.request_commitment(),
            prior_head.custody_commitment(),
            prior_head.next_page_ordinal(),
            0,
            true,
            None,
            empty_source,
            empty_target,
            prior_head.occurrence_commitment(),
            0,
            None,
            0,
            charge,
            Some(DraftMarkerAdmissionCleanupCursorV1::new(
                DraftMarkerAdmissionTreeV1::SourceOrder,
                None,
            )),
        )
    };
    let provisional =
        make_head(provisional_charge).map_err(|_| SyndicMutationError::IdentityCollision)?;
    let prior_metadata = encoded_head_record_charge(&owner, &prior_head)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_metadata = encoded_head_record_charge(&owner, &provisional)
        .and_then(|bytes| {
            encoded_receipt_record_charge(&receipt_key, &receipt).and_then(|receipt_bytes| {
                bytes
                    .checked_add(receipt_bytes)
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
            })
        })
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let successor_charge = prior_head
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
    let head = make_head(successor_charge).map_err(|_| SyndicMutationError::IdentityCollision)?;
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
    Ok(PreparedDraftMarkerWriterTerminalV1 {
        capacity,
        head,
        receipt,
    })
}

pub(crate) fn draft_marker_writer_terminal_is_exact_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    terminal_digest: DraftPieceDigestV1,
) -> Result<bool, SyndicMutationError> {
    let binding = admission.binding();
    let owner = binding.owner();
    let Some(head) = point::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)? else {
        return Ok(false);
    };
    let empty_source =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder);
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    if head.lifecycle() != DraftMarkerAdmissionLifecycleV1::TerminalCleanup
        || head.home_generation() != binding.home_generation()
        || head.owner() != owner
        || head.source_root() != empty_source
        || head.target_root() != empty_target
        || head.occurrence_commitment() != binding.occurrence_commitment()
        || head.unassigned_count() != 0
        || head.remaining_builder_count() != 0
        || head.charge().associations() > admission.remaining_count()
        || head.cleanup_cursor().is_none()
    {
        return Ok(false);
    }
    let command = staging_terminal_command(owner, terminal_digest);
    let key = DraftMarkerAdmissionReceiptKeyV1::new(owner, command);
    let Some(receipt) = point::<DraftMarkerAdmissionReceiptsFamily>(reader, &key)? else {
        return Ok(false);
    };
    Ok(terminal_receipt_is_exact(&head, key, &receipt))
}

pub(crate) fn draft_marker_writer_settlement_is_exact_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    admission: DraftMarkerWriterAdmissionV1,
    protection_published: bool,
) -> Result<bool, SyndicMutationError> {
    let binding = admission.binding();
    let owner = binding.owner();
    let head = point::<DraftMarkerAdmissionHeadsFamily>(reader, &owner)?;
    let empty_target =
        canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId);
    if !admission.is_empty()
        || head.as_ref().is_some_and(|head| {
            head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Settled
                || head.home_generation() != binding.home_generation()
                || head.owner() != owner
                || head.target_root() != empty_target
                || head.occurrence_commitment() != binding.occurrence_commitment()
                || head.unassigned_count() != 0
                || head.remaining_builder_count() != 0
                || head.charge().associations() != 0
                || head.cleanup_cursor().is_some()
        })
    {
        return Ok(false);
    }
    if protection_published && let Some(range) = binding.allocation_range() {
        let protection = required::<DraftImageLabelProtectionHeadsFamily>(
            reader,
            &binding.protection().thread_id(),
        )?;
        if !protection.protected_maximum().contains(range.last()) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn staging_terminal_command(
    owner: DraftMarkerAdmissionOwnerV1,
    staging_terminal_digest: DraftPieceDigestV1,
) -> DraftMarkerAdmissionCommandIdV1 {
    let mut command_hasher = Sha256::new();
    command_hasher.update(STAGING_TERMINAL_COMMAND_DOMAIN);
    command_hasher.update(owner.draft_id().as_bytes());
    command_hasher.update(owner.session_id().as_bytes());
    command_hasher.update(owner.operation_id().as_bytes());
    command_hasher.update(staging_terminal_digest.as_bytes());
    let command_digest: [u8; 32] = command_hasher.finalize().into();
    let mut command_bytes = [0_u8; 16];
    command_bytes.copy_from_slice(&command_digest[..16]);
    DraftMarkerAdmissionCommandIdV1::from_bytes(command_bytes)
}

pub(crate) fn contribute_draft_marker_writer_terminal_v1(
    prepared: PreparedDraftMarkerWriterTerminalV1,
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
) -> Result<(), SyndicMutationError> {
    mutations.put::<DraftMarkerAdmissionReceiptsCodec>(
        &DraftMarkerAdmissionReceiptKeyV1::new(
            prepared.receipt.owner(),
            prepared.receipt.command_id(),
        ),
        &prepared.receipt,
    )?;
    mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
    mutations.put::<DraftMarkerAdmissionCapacityCodec>(
        &DraftMarkerAdmissionCapacityKeyV1,
        &prepared.capacity,
    )?;
    Ok(())
}
