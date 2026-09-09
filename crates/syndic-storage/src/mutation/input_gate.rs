use beryl_home_store::{MutationBuildError, ReconciliationReservation};

use super::*;
use crate::{NonIdleGateSourceRecord, record::non_idle_gate_source_matches};

pub(crate) fn current_input_gate(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: &SyndicThreadId,
) -> Result<Option<InputGateRecord>, SyndicMutationError> {
    let gate = point::<InputGatesFamily>(reader, thread_id)?;
    let source = point::<NonIdleGateSourcesFamily>(reader, thread_id)?;
    if !non_idle_gate_source_matches(*thread_id, gate.as_ref(), source.as_ref()) {
        return Err(SyndicMutationError::NonIdleGateSourceMismatch);
    }
    Ok(gate)
}

pub(super) fn required_input_gate(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: &SyndicThreadId,
) -> Result<InputGateRecord, SyndicMutationError> {
    current_input_gate(reader, thread_id)?.ok_or(SyndicMutationError::RequiredRecordMissing {
        family: InputGatesFamily::NAME,
    })
}

pub(crate) fn reserve_input_gate(
    reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
) -> Result<(), MutationBuildError> {
    reservation.reserve_records::<InputGatesCodec>(1)?;
    reservation.reserve_records::<NonIdleGateSourcesCodec>(1)
}

pub(crate) fn put_input_gate(
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    gate: &InputGateRecord,
) -> Result<(), MutationBuildError> {
    mutations.put::<InputGatesCodec>(&gate.thread_id(), gate)?;
    match NonIdleGateSourceRecord::for_gate(gate) {
        Some(source) => mutations.put::<NonIdleGateSourcesCodec>(&source.thread_id(), &source),
        None => mutations.delete::<NonIdleGateSourcesCodec>(&gate.thread_id()),
    }
}

pub(crate) fn delete_input_gate(
    mutations: &mut MutationBuilder<'_, SyndicDomain>,
    thread_id: &SyndicThreadId,
) -> Result<(), MutationBuildError> {
    mutations.delete::<InputGatesCodec>(thread_id)?;
    mutations.delete::<NonIdleGateSourcesCodec>(thread_id)
}
