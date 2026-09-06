use std::any::TypeId;

use beryl_model::{
    FirstAcceptancePromotionSuccessorV1, SealedAssetReferenceSetProof, SyndicAcceptedInputId,
    SyndicDraftId,
};
use sha2::{Digest, Sha256};

use crate::{
    CodecOperation, MutationBuildError, ReadError, ReadStage, ReconciliationReader, RecordCodec,
    StorageDomain,
    command::MaterializedDomainDescriptor,
    domain::{DomainOwnerId, RegisteredDomain},
};

mod execution;

pub(crate) use execution::{
    FirstAcceptancePromotionAssetResult, FirstAcceptancePromotionDescriptor,
    FirstAcceptancePromotionExecution, FirstAcceptancePromotionReservation,
    FirstAcceptancePromotionSourceResult,
};

const FIRST_ACCEPTANCE_PROMOTION_FIXED_BYTES: usize = 512;
const FIRST_ACCEPTANCE_PROMOTION_CORRELATION_BYTES: usize = 184;
const FIRST_ACCEPTANCE_PROMOTION_SEED_BYTES: usize = 160;
const FIRST_ACCEPTANCE_PROMOTION_POINT_BYTES: usize = 256;
pub(crate) const FIRST_ACCEPTANCE_PROMOTION_COLLISION_FACT_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FirstAcceptancePromotionAdmission {
    MarkerFree,
    AssetTransferRequired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FirstAcceptancePromotionObservation {
    Authenticated(FirstAcceptancePromotionSuccessorV1),
    Unresolved,
    Collision,
}

pub trait FirstAcceptancePromotionSource<D: StorageDomain>: Send + Sync + 'static {
    fn authenticate(
        reader: &ReconciliationReader<'_, D>,
    ) -> Result<FirstAcceptancePromotionObservation, D::ValidationError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstAcceptancePromotionAssetSeed {
    pub draft_id: SyndicDraftId,
    pub accepted_input_id: SyndicAcceptedInputId,
    pub asset_reference_set: SealedAssetReferenceSetProof,
}

pub struct FirstAcceptancePromotionAssetPlan<K, V> {
    pub original_draft: K,
    pub original_accepted: K,
    pub submitted: K,
    pub expected_submitted: V,
}

pub trait FirstAcceptancePromotionAssetAdapter<D: StorageDomain>: Send + Sync + 'static {
    type OwnerHead: RecordCodec<D>;

    const MAX_DECODED_BYTES: usize;

    fn derive_plan(
        seed: &FirstAcceptancePromotionAssetSeed,
        correlation: &FirstAcceptancePromotionSuccessorV1,
    ) -> Option<
        FirstAcceptancePromotionAssetPlan<
            <Self::OwnerHead as RecordCodec<D>>::Key,
            <Self::OwnerHead as RecordCodec<D>>::Value,
        >,
    >;
}

pub(crate) struct FirstAcceptancePromotionSourceReservation {
    pub(crate) admission: FirstAcceptancePromotionAdmission,
    pub(crate) domain: &'static str,
    pub(crate) owner: DomainOwnerId,
    pub(crate) authenticate: SourceAuthenticator,
}

pub(crate) struct FirstAcceptancePromotionAssetReservation {
    pub(crate) seed: FirstAcceptancePromotionAssetSeed,
    pub(crate) domain: &'static str,
    pub(crate) owner: DomainOwnerId,
    pub(crate) codec_type: TypeId,
    pub(crate) execute: AssetExecutor,
}

pub(crate) type SourceAuthenticator = fn(
    &fjall::Snapshot,
    &RegisteredDomain,
    &MaterializedDomainDescriptor,
) -> Result<
    FirstAcceptancePromotionObservation,
    crate::domain::callback::ErasedCallbackError,
>;

pub(crate) type AssetExecutor =
    fn(
        &fjall::Snapshot,
        &RegisteredDomain,
        FirstAcceptancePromotionAssetSeed,
        FirstAcceptancePromotionSuccessorV1,
    ) -> Result<AssetExecution, crate::domain::callback::ErasedCallbackError>;

pub(crate) enum AssetExecution {
    Exact {
        points: [Option<FirstAcceptancePromotionPointFact>; 3],
    },
    Collision {
        points: [Option<FirstAcceptancePromotionPointFact>; 3],
    },
}

#[derive(Clone, Copy)]
pub(crate) struct FirstAcceptancePromotionPointFact {
    pub(crate) key_digest: [u8; 32],
    pub(crate) current_digest: Option<[u8; 32]>,
    pub(crate) expected_digest: Option<[u8; 32]>,
    pub(crate) result: FirstAcceptancePromotionPointResult,
}

#[derive(Clone, Copy)]
pub(crate) enum FirstAcceptancePromotionPointResult {
    Absent,
    Present,
    Rejected,
    Mismatch,
}

pub(crate) fn reserve_source<D, S>(
    admission: FirstAcceptancePromotionAdmission,
) -> Result<(FirstAcceptancePromotionReservation, usize), MutationBuildError>
where
    D: StorageDomain,
    S: FirstAcceptancePromotionSource<D>,
{
    let bytes = FIRST_ACCEPTANCE_PROMOTION_FIXED_BYTES
        .checked_add(FIRST_ACCEPTANCE_PROMOTION_CORRELATION_BYTES)
        .and_then(|bytes| bytes.checked_add(FIRST_ACCEPTANCE_PROMOTION_COLLISION_FACT_BYTES))
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    Ok((
        FirstAcceptancePromotionReservation::Source(FirstAcceptancePromotionSourceReservation {
            admission,
            domain: D::NAME,
            owner: DomainOwnerId::of::<D>(),
            authenticate: authenticate_source::<D, S>,
        }),
        bytes,
    ))
}

pub(crate) fn reserve_asset<D, A>(
    seed: FirstAcceptancePromotionAssetSeed,
) -> Result<(FirstAcceptancePromotionReservation, usize), MutationBuildError>
where
    D: StorageDomain,
    A: FirstAcceptancePromotionAssetAdapter<D>,
    <A::OwnerHead as RecordCodec<D>>::Value: Eq,
{
    let max_stored = <A::OwnerHead as RecordCodec<D>>::MAX_VALUE_BYTES
        .checked_add(crate::RECORD_VERSION_BYTES)
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    let per_point = FIRST_ACCEPTANCE_PROMOTION_POINT_BYTES
        .checked_add(<A::OwnerHead as RecordCodec<D>>::MAX_KEY_BYTES)
        .and_then(|bytes| bytes.checked_add(max_stored))
        .and_then(|bytes| bytes.checked_add(A::MAX_DECODED_BYTES))
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    let bytes = FIRST_ACCEPTANCE_PROMOTION_FIXED_BYTES
        .checked_add(FIRST_ACCEPTANCE_PROMOTION_SEED_BYTES)
        .and_then(|bytes| bytes.checked_add(FIRST_ACCEPTANCE_PROMOTION_CORRELATION_BYTES))
        .and_then(|bytes| bytes.checked_add(FIRST_ACCEPTANCE_PROMOTION_COLLISION_FACT_BYTES))
        .and_then(|bytes| {
            per_point
                .checked_mul(3)
                .and_then(|points| bytes.checked_add(points))
        })
        .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
    Ok((
        FirstAcceptancePromotionReservation::Asset(FirstAcceptancePromotionAssetReservation {
            seed,
            domain: D::NAME,
            owner: DomainOwnerId::of::<D>(),
            codec_type: TypeId::of::<A::OwnerHead>(),
            execute: execute_asset::<D, A>,
        }),
        bytes,
    ))
}

fn authenticate_source<D, S>(
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    descriptor: &MaterializedDomainDescriptor,
) -> Result<FirstAcceptancePromotionObservation, crate::domain::callback::ErasedCallbackError>
where
    D: StorageDomain,
    S: FirstAcceptancePromotionSource<D>,
{
    S::authenticate(&ReconciliationReader::new(snapshot, domain, descriptor))
        .map_err(crate::domain::callback::ErasedCallbackError::from_typed)
}

fn execute_asset<D, A>(
    snapshot: &fjall::Snapshot,
    domain: &RegisteredDomain,
    seed: FirstAcceptancePromotionAssetSeed,
    correlation: FirstAcceptancePromotionSuccessorV1,
) -> Result<AssetExecution, crate::domain::callback::ErasedCallbackError>
where
    D: StorageDomain,
    A: FirstAcceptancePromotionAssetAdapter<D>,
    <A::OwnerHead as RecordCodec<D>>::Value: Eq,
{
    let Some(plan) = A::derive_plan(&seed, &correlation) else {
        return Ok(AssetExecution::Collision { points: [None; 3] });
    };
    let original_draft =
        match crate::read::encode_stored_key::<D, A::OwnerHead>(&plan.original_draft) {
            Ok(key) => key,
            Err(error) if derived_rejection(&error) => {
                return Ok(AssetExecution::Collision { points: [None; 3] });
            }
            Err(error) => {
                return Err(access(error));
            }
        };
    let original_accepted =
        match crate::read::encode_stored_key::<D, A::OwnerHead>(&plan.original_accepted) {
            Ok(key) => key,
            Err(error) if derived_rejection(&error) => {
                return Ok(AssetExecution::Collision { points: [None; 3] });
            }
            Err(error) => {
                return Err(access(error));
            }
        };
    let submitted = match crate::read::encode_stored_key::<D, A::OwnerHead>(&plan.submitted) {
        Ok(key) => key,
        Err(error) if derived_rejection(&error) => {
            return Ok(AssetExecution::Collision { points: [None; 3] });
        }
        Err(error) => {
            return Err(access(error));
        }
    };
    let expected = match crate::read::encode_value::<D, A::OwnerHead>(&plan.expected_submitted) {
        Ok(value) => value,
        Err(error) if derived_rejection(&error) => {
            return Ok(AssetExecution::Collision { points: [None; 3] });
        }
        Err(error) => {
            return Err(access(error));
        }
    };
    let key_decoded = [
        <A::OwnerHead as RecordCodec<D>>::decoded_key_bytes(&original_draft, &plan.original_draft),
        <A::OwnerHead as RecordCodec<D>>::decoded_key_bytes(
            &original_accepted,
            &plan.original_accepted,
        ),
        <A::OwnerHead as RecordCodec<D>>::decoded_key_bytes(&submitted, &plan.submitted),
    ];
    let expected_decoded =
        key_decoded[2].saturating_add(<A::OwnerHead as RecordCodec<D>>::decoded_value_bytes(
            &expected[crate::RECORD_VERSION_BYTES..],
            &plan.expected_submitted,
        ));
    if key_decoded
        .into_iter()
        .any(|bytes| bytes > A::MAX_DECODED_BYTES)
        || expected_decoded > A::MAX_DECODED_BYTES
        || original_draft == original_accepted
        || original_draft == submitted
        || original_accepted == submitted
    {
        return Ok(AssetExecution::Collision { points: [None; 3] });
    }
    let Some(family_slot) = domain.family_slot(<A::OwnerHead as RecordCodec<D>>::FAMILY) else {
        return Ok(AssetExecution::Collision { points: [None; 3] });
    };
    let family = &domain.families[family_slot];
    if family.codec_type != TypeId::of::<A::OwnerHead>() {
        return Ok(AssetExecution::Collision { points: [None; 3] });
    }
    let mut points = [
        Some(point_fact(&original_draft, None)),
        Some(point_fact(&original_accepted, None)),
        Some(point_fact(&submitted, Some(&expected))),
    ];
    let draft = current::<D, A::OwnerHead>(
        snapshot,
        family,
        &original_draft,
        key_decoded[0],
        A::MAX_DECODED_BYTES,
    )?;
    observe(&mut points[0], &draft);
    if !matches!(&draft, CurrentRecord::Absent) {
        return Ok(AssetExecution::Collision { points });
    }
    let accepted = current::<D, A::OwnerHead>(
        snapshot,
        family,
        &original_accepted,
        key_decoded[1],
        A::MAX_DECODED_BYTES,
    )?;
    observe(&mut points[1], &accepted);
    if !matches!(&accepted, CurrentRecord::Absent) {
        return Ok(AssetExecution::Collision { points });
    }
    let submitted = current::<D, A::OwnerHead>(
        snapshot,
        family,
        &submitted,
        key_decoded[2],
        A::MAX_DECODED_BYTES,
    )?;
    observe(&mut points[2], &submitted);
    match submitted {
        CurrentRecord::Present { value, .. } if value == plan.expected_submitted => {
            Ok(AssetExecution::Exact { points })
        }
        CurrentRecord::Rejected { .. } => Ok(AssetExecution::Collision { points }),
        _ => {
            if let Some(point) = &mut points[2] {
                point.result = FirstAcceptancePromotionPointResult::Mismatch;
            }
            Ok(AssetExecution::Collision { points })
        }
    }
}

fn derived_rejection(error: &ReadError) -> bool {
    matches!(
        error,
        ReadError::Codec {
            operation: CodecOperation::EncodeKey | CodecOperation::EncodeValue,
            ..
        } | ReadError::InvalidKeySize { .. }
            | ReadError::BoundExceeded { .. }
    )
}

fn access(error: ReadError) -> crate::domain::callback::ErasedCallbackError {
    crate::domain::callback::ErasedCallbackError::Access(crate::DomainCallbackSource::Read(error))
}

enum CurrentRecord<V> {
    Present { value: V, digest: [u8; 32] },
    Absent,
    Rejected { digest: [u8; 32] },
}

fn point_fact(key: &[u8], expected: Option<&[u8]>) -> FirstAcceptancePromotionPointFact {
    FirstAcceptancePromotionPointFact {
        key_digest: Sha256::digest(key).into(),
        current_digest: None,
        expected_digest: expected.map(|expected| Sha256::digest(expected).into()),
        result: FirstAcceptancePromotionPointResult::Absent,
    }
}

fn observe<V>(fact: &mut Option<FirstAcceptancePromotionPointFact>, current: &CurrentRecord<V>) {
    let Some(fact) = fact else {
        return;
    };
    match current {
        CurrentRecord::Absent => fact.result = FirstAcceptancePromotionPointResult::Absent,
        CurrentRecord::Present { digest, .. } => {
            fact.current_digest = Some(*digest);
            fact.result = FirstAcceptancePromotionPointResult::Present;
        }
        CurrentRecord::Rejected { digest } => {
            fact.current_digest = Some(*digest);
            fact.result = FirstAcceptancePromotionPointResult::Rejected;
        }
    }
}

fn current<D, R>(
    snapshot: &fjall::Snapshot,
    family: &crate::domain::RegisteredFamily,
    key: &[u8],
    key_decoded_bytes: usize,
    maximum_decoded_bytes: usize,
) -> Result<CurrentRecord<R::Value>, crate::domain::callback::ErasedCallbackError>
where
    D: StorageDomain,
    R: RecordCodec<D>,
{
    let Some(point) = snapshot.point(&family.keyspace, key).map_err(|source| {
        access(ReadError::Storage {
            stage: ReadStage::PointSize,
            source: Box::new(source),
        })
    })?
    else {
        return Ok(CurrentRecord::Absent);
    };
    let actual = usize::try_from(point.stored_value_len()).expect("u32 always fits usize");
    if actual > family.max_stored_value_bytes {
        return Err(access(ReadError::InvalidStoredValueSize {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: family.max_stored_value_bytes,
            actual,
        }));
    }
    let pair = point.acquire().map_err(|source| {
        access(ReadError::Storage {
            stage: ReadStage::PointValue,
            source: Box::new(source),
        })
    })?;
    let value =
        crate::read::decode_record_envelope::<D, R>(pair.key(), pair.value()).map_err(access)?;
    let decoded = key_decoded_bytes.saturating_add(<R as RecordCodec<D>>::decoded_value_bytes(
        &pair.value()[crate::RECORD_VERSION_BYTES..],
        &value,
    ));
    if decoded > maximum_decoded_bytes {
        return Ok(CurrentRecord::Rejected {
            digest: Sha256::digest(pair.value()).into(),
        });
    }
    Ok(CurrentRecord::Present {
        value,
        digest: Sha256::digest(pair.value()).into(),
    })
}

pub(crate) fn correlation_digest(correlation: &FirstAcceptancePromotionSuccessorV1) -> [u8; 32] {
    let mut encoded = [0; FIRST_ACCEPTANCE_PROMOTION_CORRELATION_BYTES];
    encoded[..16].copy_from_slice(correlation.accepted_input_id().as_bytes());
    encoded[16..32].copy_from_slice(correlation.submitted_item_id().as_bytes());
    if let Some(proof) = correlation.asset_reference_set() {
        encoded[32] = 1;
        encoded[33..49].copy_from_slice(proof.set_id().as_bytes());
        encoded[49..81].copy_from_slice(&proof.sequential().marker_digest());
        encoded[81..89].copy_from_slice(&proof.sequential().marker_count().to_be_bytes());
        encoded[89..97].copy_from_slice(
            &proof
                .sequential()
                .maximum_image_label()
                .map_or(0, beryl_model::ImageLabelOrdinal::get)
                .to_be_bytes(),
        );
        encoded[97..129].copy_from_slice(&proof.ordered_assets().marker_asset_digest());
        encoded[129..137].copy_from_slice(&proof.ordered_assets().marker_count().to_be_bytes());
        encoded[137..145].copy_from_slice(&proof.entry_frontier().to_be_bytes());
        encoded[145..177].copy_from_slice(&proof.asset_chain_digest().as_bytes());
    }
    Sha256::digest(encoded).into()
}
