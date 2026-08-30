use std::num::NonZeroU64;

use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, ReadError, ReconciliationReservation,
};

use crate::{SyndicStorage, codec::family_point_limit, domain::SyndicDomain};

use super::index::{
    DraftMarkerAdmissionIndexPreparationErrorV1, PreparedDraftMarkerAdmissionIndexSuccessorV1,
    prepare_draft_marker_admission_index_successor_v1,
};
use super::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
    DraftMarkerAdmissionCapacityCodec, DraftMarkerAdmissionCapacityFamily,
    DraftMarkerAdmissionCapacityKeyV1, DraftMarkerAdmissionCapacityV1,
    DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionHeadsCodec,
    DraftMarkerAdmissionHeadsFamily, DraftMarkerAdmissionLifecycleV1,
    DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionReceiptKeyV1,
    DraftMarkerAdmissionReceiptTransitionV1, DraftMarkerAdmissionReceiptsCodec,
    DraftMarkerAdmissionReceiptsFamily, DraftMarkerAdmissionReplayReceiptV1,
    DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionSchemaErrorV1,
    DraftMarkerAdmissionTreeV1, DraftMarkerLabelReadinessProvenPageV1,
    canonical_empty_draft_marker_admission_root_v1,
    checked_draft_marker_admission_command_charge_v1, encoded_capacity_key_charge,
    encoded_capacity_record_charge, encoded_head_key_charge, encoded_head_record_charge,
    encoded_receipt_key_charge, encoded_receipt_record_charge,
};

#[derive(Clone)]
pub(crate) struct DraftMarkerAdmissionPublicationSeedV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    home_generation: NonZeroU64,
    request_commitment: DraftMarkerAdmissionDigestV1,
    custody_commitment: DraftMarkerAdmissionDigestV1,
    occurrence_commitment: DraftMarkerAdmissionDigestV1,
    source_head_bytes: Box<[u8]>,
    target_head_bytes: Box<[u8]>,
}

impl DraftMarkerAdmissionPublicationSeedV1 {
    pub(crate) fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        home_generation: NonZeroU64,
        request_commitment: DraftMarkerAdmissionDigestV1,
        custody_commitment: DraftMarkerAdmissionDigestV1,
        occurrence_commitment: DraftMarkerAdmissionDigestV1,
        source_head_bytes: impl Into<Box<[u8]>>,
        target_head_bytes: impl Into<Box<[u8]>>,
    ) -> Self {
        Self {
            owner,
            home_generation,
            request_commitment,
            custody_commitment,
            occurrence_commitment,
            source_head_bytes: source_head_bytes.into(),
            target_head_bytes: target_head_bytes.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum DraftMarkerAdmissionPublicationErrorV1 {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Schema(#[from] DraftMarkerAdmissionSchemaErrorV1),
    #[error("{0}")]
    Index(DraftMarkerAdmissionIndexPreparationErrorV1),
    #[error("draft-marker admission publication authority disagrees")]
    Authority,
    #[error("draft-marker admission publication revision overflowed")]
    RevisionOverflow,
    #[error("draft-marker admission publication retained charge disagrees")]
    Charge,
}

impl From<DraftMarkerAdmissionIndexPreparationErrorV1> for DraftMarkerAdmissionPublicationErrorV1 {
    fn from(value: DraftMarkerAdmissionIndexPreparationErrorV1) -> Self {
        match value {
            DraftMarkerAdmissionIndexPreparationErrorV1::Read(error) => Self::Read(error),
            other => Self::Index(other),
        }
    }
}

impl DomainCallbackError for DraftMarkerAdmissionPublicationErrorV1 {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(error) => Ok(DomainCallbackSource::Read(error)),
            other => Err(other),
        }
    }
}

struct PublicationMutation {
    seed: DraftMarkerAdmissionPublicationSeedV1,
    page: DraftMarkerLabelReadinessProvenPageV1,
    command_limit: u64,
}

struct PreparedPublicationMutation {
    capacity: DraftMarkerAdmissionCapacityV1,
    head: DraftMarkerAdmissionHeadV1,
    receipt: DraftMarkerAdmissionReplayReceiptV1,
    index: PreparedDraftMarkerAdmissionIndexSuccessorV1,
}

struct PriorPublication {
    capacity: Option<DraftMarkerAdmissionCapacityV1>,
    head: Option<DraftMarkerAdmissionHeadV1>,
    receipt: Option<DraftMarkerAdmissionReplayReceiptV1>,
    read_bytes: u64,
}

impl SyndicStorage {
    #[allow(dead_code)]
    pub(crate) fn current_publish_draft_marker_admission_v1(
        &self,
        seed: DraftMarkerAdmissionPublicationSeedV1,
        page: DraftMarkerLabelReadinessProvenPageV1,
    ) -> CurrentDomainCommand {
        self.handle.current_command(PublicationMutation {
            seed,
            page,
            command_limit: DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
        })
    }
}

impl DomainMutation<SyndicDomain> for PublicationMutation {
    type Error = DraftMarkerAdmissionPublicationErrorV1;
    type Prepared = PreparedPublicationMutation;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        prepare_publication(reader, self.seed, self.page, self.command_limit)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
        reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(
            usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 6 + 2,
        )?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for node in prepared.index.deletions() {
            mutations.delete::<DraftMarkerAdmissionNodesCodec>(&node.key())?;
        }
        for node in prepared.index.puts() {
            mutations.put::<DraftMarkerAdmissionNodesCodec>(&node.key(), node)?;
        }
        mutations.put::<DraftMarkerAdmissionHeadsCodec>(&prepared.head.owner(), &prepared.head)?;
        mutations.put::<DraftMarkerAdmissionReceiptsCodec>(
            &DraftMarkerAdmissionReceiptKeyV1::new(
                prepared.receipt.owner(),
                prepared.receipt.command_id(),
            ),
            &prepared.receipt,
        )?;
        mutations.put::<DraftMarkerAdmissionCapacityCodec>(
            &DraftMarkerAdmissionCapacityKeyV1,
            &prepared.capacity,
        )?;
        Ok(())
    }
}

fn prepare_publication(
    reader: &DomainReader<'_, SyndicDomain>,
    seed: DraftMarkerAdmissionPublicationSeedV1,
    page: DraftMarkerLabelReadinessProvenPageV1,
    command_limit: u64,
) -> Result<PreparedPublicationMutation, DraftMarkerAdmissionPublicationErrorV1> {
    let prior = read_prior(reader, &seed, &page)?;
    let (source_before, target_before, association_index, head_revision, prior_charge) =
        match prior.head.as_ref() {
            Some(head) => (
                head.source_root(),
                head.target_root(),
                usize::try_from(head.ingestion_association_cursor())
                    .map_err(|_| DraftMarkerAdmissionPublicationErrorV1::Charge)?,
                checked_increment(head.revision())?,
                head.charge(),
            ),
            None => {
                if page.sealed_page().ordinal != NonZeroU64::MIN {
                    return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
                }
                (
                    canonical_empty_draft_marker_admission_root_v1(
                        DraftMarkerAdmissionTreeV1::SourceOrder,
                    ),
                    canonical_empty_draft_marker_admission_root_v1(
                        DraftMarkerAdmissionTreeV1::TargetId,
                    ),
                    0,
                    NonZeroU64::MIN,
                    DraftMarkerAdmissionRetainedChargeV1::ZERO,
                )
            }
        };
    let prior_replay_nodes = prior
        .receipt
        .as_ref()
        .map_or(&[][..], |receipt| receipt.retained_predecessor_nodes());
    let index = prepare_draft_marker_admission_index_successor_v1(
        reader,
        seed.owner,
        source_before,
        target_before,
        prior_replay_nodes,
        &page,
        association_index,
    )?;
    let receipt = DraftMarkerAdmissionReplayReceiptV1::new(
        seed.owner,
        page.page_identity(),
        page.sealed_page().ordinal,
        seed.request_commitment,
        seed.source_head_bytes.clone(),
        seed.target_head_bytes.clone(),
        source_before,
        index.source_root(),
        target_before,
        index.target_root(),
        index.retained_predecessor_nodes(),
        DraftMarkerAdmissionReceiptTransitionV1::Ingestion,
    )?;
    let provisional = build_head(
        &seed,
        &page,
        head_revision,
        association_index,
        &index,
        DraftMarkerAdmissionRetainedChargeV1::new(1, index.target_root().count(), 0),
    )?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, page.page_identity());
    let successor_metadata_bytes = encoded_head_record_charge(&seed.owner, &provisional)?
        .checked_add(encoded_receipt_record_charge(&receipt_key, &receipt)?)
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    let prior_metadata_bytes = match (&prior.head, &prior.receipt) {
        (Some(head), Some(receipt)) => encoded_head_record_charge(&seed.owner, head)?
            .checked_add(encoded_receipt_record_charge(&receipt_key, receipt)?)
            .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?,
        (None, None) => 0,
        _ => return Err(DraftMarkerAdmissionPublicationErrorV1::Authority),
    };
    let delta = index.retained_charge_delta();
    let successor_charge = prior_charge
        .checked_sub(DraftMarkerAdmissionRetainedChargeV1::new(
            0,
            0,
            prior_metadata_bytes,
        ))
        .and_then(|charge| charge.checked_add(delta.added()))
        .and_then(|charge| charge.checked_sub(delta.removed()))
        .and_then(|charge| {
            charge.checked_add(DraftMarkerAdmissionRetainedChargeV1::new(
                u64::from(prior.head.is_none()),
                0,
                successor_metadata_bytes,
            ))
        })
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    let head = build_head(
        &seed,
        &page,
        head_revision,
        association_index,
        &index,
        successor_charge,
    )?;
    let (capacity_revision, aggregate_before) = match prior.capacity.as_ref() {
        Some(capacity) => (checked_increment(capacity.revision())?, capacity.charge()),
        None => (NonZeroU64::MIN, DraftMarkerAdmissionRetainedChargeV1::ZERO),
    };
    let aggregate_charge = aggregate_before
        .checked_sub(prior_charge)
        .and_then(|charge| charge.checked_add(successor_charge))
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    enforce_limits(successor_charge, aggregate_charge)?;
    let capacity = DraftMarkerAdmissionCapacityV1::new(capacity_revision, aggregate_charge)?;
    preflight_command(&prior, &capacity, &head, &receipt, &index, command_limit)?;
    Ok(PreparedPublicationMutation {
        capacity,
        head,
        receipt,
        index,
    })
}

fn read_prior(
    reader: &DomainReader<'_, SyndicDomain>,
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
) -> Result<PriorPublication, DraftMarkerAdmissionPublicationErrorV1> {
    let capacity = reader.point::<DraftMarkerAdmissionCapacityCodec>(
        &DraftMarkerAdmissionCapacityKeyV1,
        family_point_limit::<DraftMarkerAdmissionCapacityFamily>(),
    )?;
    let mut read_bytes = match capacity.as_ref() {
        Some(value) => encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, value)?,
        None => encoded_capacity_key_charge(&DraftMarkerAdmissionCapacityKeyV1)?,
    };
    let head = reader.point::<DraftMarkerAdmissionHeadsCodec>(
        &seed.owner,
        family_point_limit::<DraftMarkerAdmissionHeadsFamily>(),
    )?;
    read_bytes = read_bytes
        .checked_add(match head.as_ref() {
            Some(value) => encoded_head_record_charge(&seed.owner, value)?,
            None => encoded_head_key_charge(&seed.owner)?,
        })
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    let receipt = match head.as_ref() {
        Some(head) => {
            authenticate_head(seed, page, head)?;
            let command = head
                .selected_receipt()
                .ok_or(DraftMarkerAdmissionPublicationErrorV1::Authority)?;
            let key = DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, command);
            let receipt = reader.point::<DraftMarkerAdmissionReceiptsCodec>(
                &key,
                family_point_limit::<DraftMarkerAdmissionReceiptsFamily>(),
            )?;
            read_bytes = read_bytes
                .checked_add(match receipt.as_ref() {
                    Some(value) => encoded_receipt_record_charge(&key, value)?,
                    None => encoded_receipt_key_charge(&key)?,
                })
                .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
            let receipt = receipt.ok_or(DraftMarkerAdmissionPublicationErrorV1::Authority)?;
            authenticate_receipt(seed, page, head, command, &receipt)?;
            Some(receipt)
        }
        None => {
            let key = DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, page.page_identity());
            let receipt = reader.point::<DraftMarkerAdmissionReceiptsCodec>(
                &key,
                family_point_limit::<DraftMarkerAdmissionReceiptsFamily>(),
            )?;
            read_bytes = read_bytes
                .checked_add(match receipt.as_ref() {
                    Some(value) => encoded_receipt_record_charge(&key, value)?,
                    None => encoded_receipt_key_charge(&key)?,
                })
                .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
            if receipt.is_some() {
                return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
            }
            None
        }
    };
    if capacity.is_none() && head.is_some() {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(PriorPublication {
        capacity,
        head,
        receipt,
        read_bytes,
    })
}

fn authenticate_head(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    if head.owner() != seed.owner
        || head.home_generation() != seed.home_generation
        || head.lifecycle() != DraftMarkerAdmissionLifecycleV1::Ingesting
        || head.request_commitment() != seed.request_commitment
        || head.custody_commitment() != seed.custody_commitment
        || head.occurrence_commitment() != seed.occurrence_commitment
        || head.next_page_ordinal() != page.sealed_page().ordinal
        || head.charge().associations() != head.target_root().count()
    {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(())
}

fn authenticate_receipt(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
    head: &DraftMarkerAdmissionHeadV1,
    selected_command: super::DraftMarkerAdmissionCommandIdV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    if receipt.owner() != seed.owner
        || receipt.command_id() != selected_command
        || selected_command != page.page_identity()
        || receipt.page_ordinal() != page.sealed_page().ordinal
        || receipt.request_commitment() != seed.request_commitment
        || receipt.source_head_bytes() != seed.source_head_bytes.as_ref()
        || receipt.target_head_bytes() != seed.target_head_bytes.as_ref()
        || receipt.source_after() != head.source_root()
        || receipt.target_after() != head.target_root()
        || receipt.transition() != DraftMarkerAdmissionReceiptTransitionV1::Ingestion
    {
        return Err(DraftMarkerAdmissionPublicationErrorV1::Authority);
    }
    Ok(())
}

fn build_head(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
    revision: NonZeroU64,
    association_index: usize,
    index: &PreparedDraftMarkerAdmissionIndexSuccessorV1,
    charge: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionHeadV1::new(
        seed.owner,
        revision,
        seed.home_generation,
        DraftMarkerAdmissionLifecycleV1::Ingesting,
        seed.request_commitment,
        seed.custody_commitment,
        page.sealed_page().ordinal,
        u64::try_from(association_index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?,
        false,
        Some(page.page_identity()),
        index.source_root(),
        index.target_root(),
        seed.occurrence_commitment,
        index.target_root().count(),
        None,
        0,
        charge,
        None,
    )
}

fn preflight_command(
    prior: &PriorPublication,
    capacity: &DraftMarkerAdmissionCapacityV1,
    head: &DraftMarkerAdmissionHeadV1,
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
    index: &PreparedDraftMarkerAdmissionIndexSuccessorV1,
    command_limit: u64,
) -> Result<(), DraftMarkerAdmissionPublicationErrorV1> {
    let footprint = index.footprint();
    let capacity_bytes =
        encoded_capacity_record_charge(&DraftMarkerAdmissionCapacityKeyV1, capacity)?;
    let head_bytes = encoded_head_record_charge(&head.owner(), head)?;
    let receipt_bytes = encoded_receipt_record_charge(
        &DraftMarkerAdmissionReceiptKeyV1::new(receipt.owner(), receipt.command_id()),
        receipt,
    )?;
    let write_bytes = footprint
        .write_bytes()
        .checked_add(capacity_bytes)
        .and_then(|bytes| bytes.checked_add(head_bytes))
        .and_then(|bytes| bytes.checked_add(receipt_bytes))
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    let command_bytes = checked_draft_marker_admission_command_charge_v1([
        prior
            .read_bytes
            .checked_add(footprint.read_bytes())
            .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?,
        write_bytes,
        footprint.delete_bytes(),
    ])?;
    if command_bytes > command_limit {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge.into());
    }
    Ok(())
}

fn enforce_limits(
    operation: DraftMarkerAdmissionRetainedChargeV1,
    aggregate: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
    if operation.heads() != 1
        || !operation.fits(super::DraftMarkerAdmissionLimitsV1::PRODUCTION)
        || !aggregate.fits(super::DraftMarkerAdmissionLimitsV1::PRODUCTION)
    {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded);
    }
    Ok(())
}

fn checked_increment(
    value: NonZeroU64,
) -> Result<NonZeroU64, DraftMarkerAdmissionPublicationErrorV1> {
    value
        .get()
        .checked_add(1)
        .and_then(NonZeroU64::new)
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::RevisionOverflow)
}

#[cfg(feature = "test-faults")]
mod test_fixture;

#[cfg(feature = "test-faults")]
pub use test_fixture::{
    DraftMarkerAdmissionPublicationFixtureV1, DraftMarkerAdmissionPublicationSnapshotV1,
};
