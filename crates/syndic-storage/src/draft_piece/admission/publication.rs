use std::num::NonZeroU64;

use beryl_home_store::{
    CommandError, CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation,
    DomainReader, MutationBuildError, MutationBuilder, ReadError, ReconciliationReservation,
};

use crate::{SyndicStorage, codec::family_point_limit, domain::SyndicDomain};

use super::index::{
    DraftMarkerAdmissionIndexPreparationErrorV1, PreparedDraftMarkerAdmissionIndexSuccessorV1,
    prepare_draft_marker_admission_index_successor_v1,
};
use super::readiness_source::{
    DraftMarkerLabelReadinessRequestAuthorityV1, page_closure_bytes, request_authority_is_exact,
};
use super::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
    DraftMarkerAdmissionAssignmentContinuationV1, DraftMarkerAdmissionCapacityCodec,
    DraftMarkerAdmissionCapacityFamily, DraftMarkerAdmissionCapacityKeyV1,
    DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionHeadV1,
    DraftMarkerAdmissionHeadsCodec, DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionNodesCodec, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionReceiptKeyV1, DraftMarkerAdmissionReceiptTransitionV1,
    DraftMarkerAdmissionReceiptsCodec, DraftMarkerAdmissionReceiptsFamily,
    DraftMarkerAdmissionReplayReceiptV1, DraftMarkerAdmissionRetainedChargeV1,
    DraftMarkerAdmissionSchemaErrorV1, DraftMarkerAdmissionTreeV1,
    DraftMarkerLabelAllocationRangeV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerLabelReadinessProvenPageV1, canonical_empty_draft_marker_admission_root_v1,
    checked_draft_marker_admission_command_charge_v1, encoded_capacity_key_charge,
    encoded_capacity_record_charge, encoded_head_key_charge, encoded_head_record_charge,
    encoded_receipt_key_charge, encoded_receipt_record_charge,
};

mod progression;

use progression::{
    PageProgression, PageProgressionError, authenticate_head, authenticate_progression,
    authenticate_receipt_closure, page_progression,
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
    disposition: DraftMarkerLabelReadinessDispositionV1,
    allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
    authority: PublicationAuthority,
}

#[derive(Clone)]
enum PublicationAuthority {
    Runtime(DraftMarkerLabelReadinessRequestAuthorityV1),
    #[cfg(feature = "test-faults")]
    Fixture,
}

impl DraftMarkerAdmissionPublicationSeedV1 {
    pub(crate) fn from_page(
        page: &super::readiness_source::SealedDraftMarkerReadinessSourcePageV1,
        allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
    ) -> Self {
        let (source_head_bytes, target_head_bytes) = page_closure_bytes(page);
        Self {
            owner: page.owner,
            home_generation: page.authority.home_generation,
            request_commitment: page.authority.request_commitment(),
            custody_commitment: page.authority.custody_commitment(),
            occurrence_commitment: page.authority.occurrence_commitment(),
            source_head_bytes,
            target_head_bytes,
            disposition: page.authority.disposition,
            allocation_range,
            authority: PublicationAuthority::Runtime(page.authority.clone()),
        }
    }

    #[cfg(feature = "test-faults")]
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
            disposition: DraftMarkerLabelReadinessDispositionV1::Reuse,
            allocation_range: None,
            authority: PublicationAuthority::Fixture,
        }
    }

    pub(crate) fn owner(&self) -> DraftMarkerAdmissionOwnerV1 {
        self.owner
    }

    pub(crate) fn home_generation(&self) -> NonZeroU64 {
        self.home_generation
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum PublicationFailureClass {
    Obsolete,
    Collision,
    Refused,
    Retryable,
}

pub(super) fn classify_not_committed(error: &CommandError) -> PublicationFailureClass {
    let source = match error {
        CommandError::ContributorValidation { source, .. }
        | CommandError::ContributorReservation { source, .. }
        | CommandError::ContributorAssembly { source, .. } => Some(source.as_ref()),
        _ => None,
    };
    match source.and_then(|source| source.downcast_ref::<DraftMarkerAdmissionPublicationErrorV1>())
    {
        Some(DraftMarkerAdmissionPublicationErrorV1::ObsoletePage) => {
            PublicationFailureClass::Obsolete
        }
        Some(DraftMarkerAdmissionPublicationErrorV1::Collision) => {
            PublicationFailureClass::Collision
        }
        Some(
            DraftMarkerAdmissionPublicationErrorV1::Authority
            | DraftMarkerAdmissionPublicationErrorV1::RequestAuthority
            | DraftMarkerAdmissionPublicationErrorV1::PageIncomplete,
        ) => PublicationFailureClass::Refused,
        Some(_) => PublicationFailureClass::Retryable,
        None => PublicationFailureClass::Retryable,
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
    #[error("draft-marker admission request authority is no longer current")]
    RequestAuthority,
    #[error("draft-marker admission publication revision overflowed")]
    RevisionOverflow,
    #[error("draft-marker admission publication retained charge disagrees")]
    Charge,
    #[error("draft-marker admission page is obsolete")]
    ObsoletePage,
    #[error("draft-marker admission current page is incomplete")]
    PageIncomplete,
    #[error("draft-marker admission occupied page identity collides")]
    Collision,
}

impl From<DraftMarkerAdmissionIndexPreparationErrorV1> for DraftMarkerAdmissionPublicationErrorV1 {
    fn from(value: DraftMarkerAdmissionIndexPreparationErrorV1) -> Self {
        match value {
            DraftMarkerAdmissionIndexPreparationErrorV1::Read(error) => Self::Read(error),
            DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateSource
            | DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateTarget => Self::Collision,
            other => Self::Index(other),
        }
    }
}

impl From<PageProgressionError> for DraftMarkerAdmissionPublicationErrorV1 {
    fn from(value: PageProgressionError) -> Self {
        match value {
            PageProgressionError::Authority => Self::Authority,
            PageProgressionError::Obsolete => Self::ObsoletePage,
            PageProgressionError::PageIncomplete => Self::PageIncomplete,
            PageProgressionError::Overflow => Self::RevisionOverflow,
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
    receipt_deletion: Option<DraftMarkerAdmissionReceiptKeyV1>,
    index: PreparedDraftMarkerAdmissionIndexSuccessorV1,
}

struct PriorPublication {
    capacity: Option<DraftMarkerAdmissionCapacityV1>,
    head: Option<DraftMarkerAdmissionHeadV1>,
    receipt: Option<DraftMarkerAdmissionReplayReceiptV1>,
    receipt_key: DraftMarkerAdmissionReceiptKeyV1,
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
        reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(2)?;
        reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(
            usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 6 + 2,
        )?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(key) = prepared.receipt_deletion {
            mutations.delete::<DraftMarkerAdmissionReceiptsCodec>(&key)?;
        }
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
    match &seed.authority {
        PublicationAuthority::Runtime(authority) => {
            if !request_authority_is_exact(reader, authority)? {
                return Err(DraftMarkerAdmissionPublicationErrorV1::RequestAuthority);
            }
        }
        #[cfg(feature = "test-faults")]
        PublicationAuthority::Fixture => {}
    }
    let prior = read_prior(reader, &seed, &page)?;
    let progression = page_progression(prior.head.as_ref(), &page)?;
    authenticate_progression(&seed, &page, &prior, progression)?;
    let (source_before, target_before, head_revision, prior_charge) = match prior.head.as_ref() {
        Some(head) => (
            head.source_root(),
            head.target_root(),
            checked_increment(head.revision())?,
            head.charge(),
        ),
        None => (
            canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::SourceOrder),
            canonical_empty_draft_marker_admission_root_v1(DraftMarkerAdmissionTreeV1::TargetId),
            NonZeroU64::MIN,
            DraftMarkerAdmissionRetainedChargeV1::ZERO,
        ),
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
        progression.association_index,
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
        progression,
        &index,
        DraftMarkerAdmissionRetainedChargeV1::new(1, index.target_root().count(), 0),
    )?;
    let receipt_key = DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, page.page_identity());
    let successor_metadata_bytes = encoded_head_record_charge(&seed.owner, &provisional)?
        .checked_add(encoded_receipt_record_charge(&receipt_key, &receipt)?)
        .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?;
    let prior_metadata_bytes = match (&prior.head, &prior.receipt) {
        (Some(head), Some(receipt)) => encoded_head_record_charge(&seed.owner, head)?
            .checked_add(encoded_receipt_record_charge(&prior.receipt_key, receipt)?)
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
        progression,
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
    let receipt_deletion =
        (prior.receipt.is_some() && prior.receipt_key != receipt_key).then_some(prior.receipt_key);
    preflight_command(
        &prior,
        &capacity,
        &head,
        &receipt,
        receipt_deletion,
        &index,
        command_limit,
    )?;
    Ok(PreparedPublicationMutation {
        capacity,
        head,
        receipt,
        receipt_deletion,
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
            authenticate_head(seed, head)?;
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
            authenticate_receipt_closure(seed, head, command, &receipt)?;
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
    let receipt_key = head
        .as_ref()
        .and_then(DraftMarkerAdmissionHeadV1::selected_receipt)
        .map_or_else(
            || DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, page.page_identity()),
            |command| DraftMarkerAdmissionReceiptKeyV1::new(seed.owner, command),
        );
    Ok(PriorPublication {
        capacity,
        head,
        receipt,
        receipt_key,
        read_bytes,
    })
}

fn build_head(
    seed: &DraftMarkerAdmissionPublicationSeedV1,
    page: &DraftMarkerLabelReadinessProvenPageV1,
    revision: NonZeroU64,
    progression: PageProgression,
    index: &PreparedDraftMarkerAdmissionIndexSuccessorV1,
    charge: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionSchemaErrorV1> {
    let (lifecycle, occurrence_commitment, continuation) = if progression.final_eof {
        let continuation = match seed.disposition {
            DraftMarkerLabelReadinessDispositionV1::Reuse => {
                if seed.allocation_range.is_some() {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
                }
                DraftMarkerAdmissionAssignmentContinuationV1::reuse(None)
            }
            DraftMarkerLabelReadinessDispositionV1::Allocate => {
                let range = seed
                    .allocation_range
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?;
                if range.count() != index.target_root().count() {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
                }
                DraftMarkerAdmissionAssignmentContinuationV1::allocate(range, range.first(), None)?
            }
        };
        (
            DraftMarkerAdmissionLifecycleV1::Assigning,
            index.source_root().digest(),
            Some(continuation),
        )
    } else {
        (
            DraftMarkerAdmissionLifecycleV1::Ingesting,
            seed.occurrence_commitment,
            None,
        )
    };
    DraftMarkerAdmissionHeadV1::new(
        seed.owner,
        revision,
        seed.home_generation,
        lifecycle,
        seed.request_commitment,
        seed.custody_commitment,
        progression.next_page_ordinal,
        progression.next_association_cursor,
        progression.final_eof,
        Some(page.page_identity()),
        index.source_root(),
        index.target_root(),
        occurrence_commitment,
        index.target_root().count(),
        continuation,
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
    receipt_deletion: Option<DraftMarkerAdmissionReceiptKeyV1>,
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
    let receipt_delete_bytes = match (receipt_deletion, prior.receipt.as_ref()) {
        (Some(key), Some(receipt)) => encoded_receipt_record_charge(&key, receipt)?,
        (None, _) => 0,
        (Some(_), None) => return Err(DraftMarkerAdmissionPublicationErrorV1::Authority),
    };
    let command_bytes = checked_draft_marker_admission_command_charge_v1([
        prior
            .read_bytes
            .checked_add(footprint.read_bytes())
            .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?,
        write_bytes,
        footprint
            .delete_bytes()
            .checked_add(receipt_delete_bytes)
            .ok_or(DraftMarkerAdmissionPublicationErrorV1::Charge)?,
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
