use beryl_home_store::DomainReader;
use beryl_model::{BindingRevision, CasNativeTurnCount, CasThreadId, SyndicThreadId};

use crate::{
    BindingRecord, BindingState, CasLineageProof, CasRepresentedPrefixProof,
    CasThreadBindingIndexRecord, CasThreadIndexRecord, NativeCasLineage, SelectedPathProof,
    StaleCasBinding, SyndicMutationError, TurnLifecycle, UsableCasBinding, codec::*,
    domain::SyndicDomain,
};

use super::super::{point, required};

pub(super) struct TransitionBase {
    pub(super) current: BindingRecord,
    pub(super) next_revision: BindingRevision,
}

pub(super) fn transition_base(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    expected_revision: BindingRevision,
    selected_path: SelectedPathProof,
) -> Result<TransitionBase, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    let head = required::<BindingHeadsFamily>(reader, &thread_id)?;
    if head.revision() != expected_revision {
        return Err(SyndicMutationError::BindingRevisionConflict {
            expected: expected_revision,
            current: head.revision(),
        });
    }
    let current = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread_id,
            revision: expected_revision,
        },
    )?;
    if current.selected_path() != selected_path
        || selected_path.tail() != thread.committed_tail()
        || selected_path.digest() != thread.selected_path_digest()
        || selected_path.thread_revision() > thread.revision()
    {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    let next_revision = expected_revision.checked_next()?;
    if point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread_id,
            revision: next_revision,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok(TransitionBase {
        current,
        next_revision,
    })
}

pub(super) fn ensure_not_active(current: &BindingRecord) -> Result<(), SyndicMutationError> {
    if matches!(current.state(), BindingState::Active(_)) {
        Err(SyndicMutationError::BindingStateConflict)
    } else {
        Ok(())
    }
}

pub(super) fn reservation(
    reader: &DomainReader<'_, SyndicDomain>,
    usable: &UsableCasBinding,
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
) -> Result<CasThreadIndexRecord, SyndicMutationError> {
    let cas_thread_id = usable.cas_thread_id();
    match point::<CasThreadIndexFamily>(reader, &CasThreadKey::Record(cas_thread_id.clone()))? {
        Some(index) if index.thread_id() != thread_id => {
            Err(SyndicMutationError::CasThreadOwnershipConflict)
        }
        Some(index) if index.retired_binding_revision().is_some() => {
            Err(SyndicMutationError::CasThreadRetired)
        }
        Some(index) => {
            if index.latest_binding_revision() >= binding_revision {
                return Err(SyndicMutationError::BindingStateConflict);
            }
            let prior = required::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: thread_id,
                    revision: index.latest_binding_revision(),
                },
            )?;
            validate_latest_membership(reader, &index)?;
            let prior_usable = match prior.state() {
                BindingState::Valid(prior) => prior,
                BindingState::Active(prior) => prior.usable(),
                BindingState::Unbound { .. } | BindingState::Stale(_) => {
                    return Err(SyndicMutationError::BindingStateConflict);
                }
            };
            validate_reservation_continuity(prior_usable, usable)?;
            Ok(index.advance(binding_revision))
        }
        None => {
            validate_new_reservation(usable)?;
            Ok(CasThreadIndexRecord::new(
                cas_thread_id.clone(),
                thread_id,
                binding_revision,
            ))
        }
    }
}

fn validate_reservation_continuity(
    prior: &UsableCasBinding,
    next: &UsableCasBinding,
) -> Result<(), SyndicMutationError> {
    if prior.execution() != next.execution()
        || prior.cas_thread_id() != next.cas_thread_id()
        || prior.tool_profile() != next.tool_profile()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    if prior.lineage() == next.lineage()
        && same_stable_prefix(prior.represented_prefix(), next.represented_prefix())
    {
        return if prior.native_turn_count() == next.native_turn_count() {
            Ok(())
        } else {
            Err(SyndicMutationError::BindingStateConflict)
        };
    }
    match next.lineage() {
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Resume,
            established_prefix,
        } if matches!(prior.lineage(), CasLineageProof::Native { .. })
            && established_prefix == prior.represented_prefix()
            && same_stable_prefix(prior.represented_prefix(), next.represented_prefix())
            && next.native_turn_count() == prior.native_turn_count() =>
        {
            Ok(())
        }
        _ => Err(SyndicMutationError::BindingPathConflict),
    }
}

fn validate_new_reservation(usable: &UsableCasBinding) -> Result<(), SyndicMutationError> {
    if usable.lineage().established_prefix() != usable.represented_prefix() {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    match usable.lineage() {
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fresh,
            ..
        } if usable.represented_prefix().tail().is_none()
            && usable.native_turn_count() == CasNativeTurnCount::ZERO =>
        {
            Ok(())
        }
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fresh,
            ..
        }
        | CasLineageProof::RecoveredInjection(_)
            if usable.native_turn_count() != CasNativeTurnCount::ZERO =>
        {
            Err(SyndicMutationError::BindingStateConflict)
        }
        CasLineageProof::RecoveredInjection(_) | CasLineageProof::Native { .. } => Ok(()),
    }
}

fn same_stable_prefix(prior: CasRepresentedPrefixProof, next: CasRepresentedPrefixProof) -> bool {
    prior.tail() == next.tail()
        && prior.digest() == next.digest()
        && prior.source_thread_revision() <= next.source_thread_revision()
}

pub(in crate::mutation) fn advance_reservation(
    reader: &DomainReader<'_, SyndicDomain>,
    cas_thread_id: &CasThreadId,
    thread_id: SyndicThreadId,
    expected_latest_revision: BindingRevision,
    next_revision: BindingRevision,
) -> Result<CasThreadIndexRecord, SyndicMutationError> {
    let index =
        point::<CasThreadIndexFamily>(reader, &CasThreadKey::Record(cas_thread_id.clone()))?
            .ok_or(SyndicMutationError::BindingStateConflict)?;
    if index.thread_id() != thread_id
        || index.retired_binding_revision().is_some()
        || index.latest_binding_revision() != expected_latest_revision
        || next_revision <= expected_latest_revision
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    validate_latest_membership(reader, &index)?;
    Ok(index.advance(next_revision))
}

pub(in crate::mutation) fn membership(
    reader: &DomainReader<'_, SyndicDomain>,
    cas_thread_id: &CasThreadId,
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
) -> Result<CasThreadBindingIndexRecord, SyndicMutationError> {
    let key = CasThreadBindingKey::Record(cas_thread_id.clone(), binding_revision);
    if point::<CasThreadBindingIndexFamily>(reader, &key)?.is_some() {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    Ok(CasThreadBindingIndexRecord::new(
        cas_thread_id.clone(),
        thread_id,
        binding_revision,
    ))
}

pub(super) fn retirement(
    reader: &DomainReader<'_, SyndicDomain>,
    stale: &StaleCasBinding,
    thread_id: SyndicThreadId,
    stale_revision: BindingRevision,
) -> Result<Option<CasThreadIndexRecord>, SyndicMutationError> {
    let cas_thread_id = stale.cas_thread_id();
    match point::<CasThreadIndexFamily>(reader, &CasThreadKey::Record(cas_thread_id.clone()))? {
        Some(index) if index.thread_id() != thread_id => {
            Err(SyndicMutationError::CasThreadOwnershipConflict)
        }
        Some(index) if index.retired_binding_revision().is_some() => {
            Err(SyndicMutationError::CasThreadRetired)
        }
        Some(index) => {
            validate_latest_membership(reader, &index)?;
            let prior = required::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: thread_id,
                    revision: index.latest_binding_revision(),
                },
            )?;
            let prior = match prior.state() {
                BindingState::Valid(usable) => usable,
                BindingState::Active(active) => active.usable(),
                BindingState::Unbound { .. } | BindingState::Stale(_) => {
                    return Err(SyndicMutationError::BindingStateConflict);
                }
            };
            if prior.execution() != stale.execution()
                || prior.cas_thread_id() != stale.cas_thread_id()
                || stale.observed_tool_profile() != Some(prior.tool_profile())
                || stale
                    .observed_prefix()
                    .is_some_and(|prefix| prefix != prior.represented_prefix())
                || stale
                    .observed_lineage()
                    .is_some_and(|lineage| lineage != prior.lineage())
                || stale.observed_native_turn_count() != Some(prior.native_turn_count())
            {
                return Err(SyndicMutationError::BindingStateConflict);
            }
            Ok(Some(index.retire(stale_revision)))
        }
        None => {
            if !valid_first_stale_position(stale) {
                return Err(SyndicMutationError::BindingStateConflict);
            }
            Ok(Some(CasThreadIndexRecord::retired(
                cas_thread_id.clone(),
                thread_id,
                stale_revision,
                stale_revision,
            )))
        }
    }
}

fn valid_first_stale_position(stale: &StaleCasBinding) -> bool {
    match stale.observed_native_turn_count() {
        None | Some(CasNativeTurnCount::ZERO) => true,
        Some(_) => {
            let Some(prefix) = stale.observed_prefix() else {
                return false;
            };
            matches!(
                stale.observed_lineage(),
                Some(CasLineageProof::Native {
                    mechanism: NativeCasLineage::Fork,
                    established_prefix,
                }) if prefix.tail().is_some() && established_prefix == prefix
            )
        }
    }
}

fn validate_latest_membership(
    reader: &DomainReader<'_, SyndicDomain>,
    index: &CasThreadIndexRecord,
) -> Result<(), SyndicMutationError> {
    let membership = required::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(
            index.cas_thread_id().clone(),
            index.latest_binding_revision(),
        ),
    )?;
    if membership.cas_thread_id() != index.cas_thread_id()
        || membership.thread_id() != index.thread_id()
        || membership.binding_revision() != index.latest_binding_revision()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    Ok(())
}

pub(super) fn validate_usable_current(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: SelectedPathProof,
    usable: &UsableCasBinding,
) -> Result<(), SyndicMutationError> {
    if usable.represented_prefix().tail().is_none()
        && usable.native_turn_count() != CasNativeTurnCount::ZERO
    {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    validate_prefix(reader, selected, usable.represented_prefix())?;
    let established = usable.lineage().established_prefix();
    validate_prefix(reader, selected, established)?;
    if established.source_thread_revision() > usable.represented_prefix().source_thread_revision()
        || !prefix_contains(reader, usable.represented_prefix(), established)?
    {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    match selected.tail() {
        None => validate_exact_selected(selected, usable.represented_prefix()),
        Some(tail) => {
            let lifecycle = required::<TurnStatesFamily>(reader, &tail)?.lifecycle();
            if lifecycle == TurnLifecycle::Pending {
                validate_exact_parent(reader, selected, usable.represented_prefix())
            } else if lifecycle.is_proven_terminal() {
                validate_exact_selected(selected, usable.represented_prefix())
            } else {
                Err(SyndicMutationError::TurnLifecycleConflict)
            }
        }
    }
}

fn validate_exact_selected(
    selected: SelectedPathProof,
    prefix: CasRepresentedPrefixProof,
) -> Result<(), SyndicMutationError> {
    if prefix.tail() == selected.tail()
        && prefix.digest() == selected.digest()
        && prefix.source_thread_revision() == selected.thread_revision()
    {
        Ok(())
    } else {
        Err(SyndicMutationError::BindingPathConflict)
    }
}

pub(super) fn validate_stale(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: SelectedPathProof,
    stale: &StaleCasBinding,
) -> Result<(), SyndicMutationError> {
    if let Some(prefix) = stale.observed_prefix() {
        if prefix.tail().is_none()
            && stale
                .observed_native_turn_count()
                .is_some_and(|count| count != CasNativeTurnCount::ZERO)
        {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        validate_prefix(reader, selected, prefix)?;
    }
    if let Some(lineage) = stale.observed_lineage() {
        let established = lineage.established_prefix();
        validate_prefix(reader, selected, established)?;
        if let Some(prefix) = stale.observed_prefix()
            && !prefix_contains(reader, prefix, established)?
        {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        if let Some(required) = lineage.recovered_loaded_generation()
            && stale.loaded_generation() != Some(required)
        {
            return Err(SyndicMutationError::BindingPathConflict);
        }
    }
    Ok(())
}

pub(super) fn validate_exact_parent(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: SelectedPathProof,
    prefix: CasRepresentedPrefixProof,
) -> Result<(), SyndicMutationError> {
    let tail = selected
        .tail()
        .ok_or(SyndicMutationError::BindingPathConflict)?;
    let turn = required::<TurnsFamily>(reader, &tail)?;
    let (expected_tail, expected_digest) = match turn.parent().turn() {
        Some(parent) => {
            let parent = required::<TurnsFamily>(reader, &parent)?;
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, crate::empty_selected_path_digest()),
    };
    if prefix.tail() == expected_tail
        && prefix.digest() == expected_digest
        && prefix.source_thread_revision() == selected.thread_revision()
    {
        Ok(())
    } else {
        Err(SyndicMutationError::BindingPathConflict)
    }
}

fn validate_prefix(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: SelectedPathProof,
    prefix: CasRepresentedPrefixProof,
) -> Result<(), SyndicMutationError> {
    if prefix.source_thread_revision() > selected.thread_revision() {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    match (selected.tail(), prefix.tail()) {
        (_, None) if prefix.digest() == crate::empty_selected_path_digest() => Ok(()),
        (_, None) | (None, Some(_)) => Err(SyndicMutationError::BindingPathConflict),
        (Some(selected_tail), Some(prefix_tail)) => {
            let selected = required::<TurnsFamily>(reader, &selected_tail)?;
            let prefix_turn = required::<TurnsFamily>(reader, &prefix_tail)?;
            if prefix_turn.chain_digest() != prefix.digest()
                || !crate::selected_path::includes_turn(
                    selected,
                    &prefix_turn,
                    |id| required::<TurnsFamily>(reader, &id),
                    |_| SyndicMutationError::BindingPathConflict,
                )?
            {
                return Err(SyndicMutationError::BindingPathConflict);
            }
            Ok(())
        }
    }
}

fn prefix_contains(
    reader: &DomainReader<'_, SyndicDomain>,
    outer: CasRepresentedPrefixProof,
    inner: CasRepresentedPrefixProof,
) -> Result<bool, SyndicMutationError> {
    match (outer.tail(), inner.tail()) {
        (_, None) => Ok(inner.digest() == crate::empty_selected_path_digest()),
        (None, Some(_)) => Ok(false),
        (Some(outer_id), Some(inner_id)) => {
            let outer_turn = required::<TurnsFamily>(reader, &outer_id)?;
            let inner_turn = required::<TurnsFamily>(reader, &inner_id)?;
            if outer_turn.chain_digest() != outer.digest()
                || inner_turn.chain_digest() != inner.digest()
            {
                return Err(SyndicMutationError::BindingPathConflict);
            }
            crate::selected_path::includes_turn(
                outer_turn,
                &inner_turn,
                |id| required::<TurnsFamily>(reader, &id),
                |_| SyndicMutationError::BindingPathConflict,
            )
        }
    }
}
