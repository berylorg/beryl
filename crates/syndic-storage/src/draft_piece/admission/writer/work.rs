use beryl_home_store::{DomainReader, PointReadLimit};

use crate::{
    SyndicMutationError,
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
    mutation::point,
};

use super::super::AdmissionWorkLedger;

pub(crate) fn draft_marker_writer_point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
    work: Option<&AdmissionWorkLedger>,
) -> Result<Option<F::Value>, SyndicMutationError> {
    let reservation = work
        .map(|work| work.reserve_point(AdmissionWorkLedger::family_maximum::<F>()))
        .transpose()
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let value = if work.is_some() {
        reader.point::<ExactCodec<F>>(
            key,
            PointReadLimit::new(AdmissionWorkLedger::family_value_limit::<F>())
                .map_err(|_| SyndicMutationError::IdentityCollision)?,
        )?
    } else {
        point::<F>(reader, key)?
    };
    if let Some(reservation) = reservation {
        let bytes = control_bytes::<F>(key, value.as_ref())?;
        reservation
            .finish(bytes, false)
            .map_err(|_| SyndicMutationError::IdentityCollision)?;
    }
    Ok(value)
}

pub(crate) fn draft_marker_writer_required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
    work: &AdmissionWorkLedger,
) -> Result<F::Value, SyndicMutationError> {
    draft_marker_writer_point::<F>(reader, key, Some(work))?
        .ok_or(SyndicMutationError::RequiredRecordMissing { family: F::NAME })
}

pub(crate) fn charge_draft_marker_writer_emit<F: Family>(
    work: &AdmissionWorkLedger,
    key: &F::Key,
    value: &F::Value,
) -> Result<(), SyndicMutationError> {
    work.charge_emit(control_bytes::<F>(key, Some(value))?, false)
        .map_err(|_| SyndicMutationError::IdentityCollision)
}

fn control_bytes<F: Family>(
    key: &F::Key,
    value: Option<&F::Value>,
) -> Result<u64, SyndicMutationError> {
    let key = F::encode_key(key).map_err(|_| SyndicMutationError::IdentityCollision)?;
    let value = value
        .map(F::encode_value)
        .transpose()
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    (key.len() as u64)
        .checked_add(value.as_ref().map_or(0, |value| value.len() as u64))
        .ok_or(SyndicMutationError::IdentityCollision)
}
