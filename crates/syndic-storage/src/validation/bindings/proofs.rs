use beryl_home_store::DomainReader;
use beryl_model::CasNativeTurnCount;

use crate::{TurnLifecycle, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use super::invariant;
use crate::validation::scan::require;

pub(super) fn validate_selected_path(
    reader: &DomainReader<'_, SyndicDomain>,
    proof: crate::SelectedPathProof,
) -> Result<(), SyndicValidationError> {
    match proof.tail() {
        Some(tail) => {
            let turn =
                require::<TurnsFamily>(reader, &tail, "binding selected-path tail is missing")?;
            if turn.chain_digest() != proof.digest() {
                return invariant("binding selected-path digest disagrees with tail");
            }
        }
        None if proof.digest() == crate::empty_selected_path_digest() => {}
        None => return invariant("empty binding path has noncanonical digest"),
    }
    Ok(())
}

fn validate_represented_prefix(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: crate::SelectedPathProof,
    prefix: crate::CasRepresentedPrefixProof,
) -> Result<(), SyndicValidationError> {
    if prefix.source_thread_revision() > selected.thread_revision() {
        return invariant("CAS represented-prefix revision exceeds its selected path");
    }
    match (selected.tail(), prefix.tail()) {
        (_, None) if prefix.digest() == crate::empty_selected_path_digest() => Ok(()),
        (_, None) => invariant("empty CAS represented prefix has noncanonical digest"),
        (None, Some(_)) => invariant("nonempty CAS represented prefix exceeds an empty path"),
        (Some(selected_tail), Some(prefix_tail)) => {
            let selected_turn = require::<TurnsFamily>(
                reader,
                &selected_tail,
                "binding selected-path tail is missing",
            )?;
            let prefix_turn = require::<TurnsFamily>(
                reader,
                &prefix_tail,
                "CAS represented-prefix tail is missing",
            )?;
            if prefix_turn.chain_digest() != prefix.digest() {
                return invariant("CAS represented-prefix digest disagrees with its tail");
            }
            if !crate::selected_path::includes_turn(
                selected_turn,
                &prefix_turn,
                |turn_id| {
                    require::<TurnsFamily>(
                        reader,
                        &turn_id,
                        "CAS represented-prefix ancestor is missing",
                    )
                },
                SyndicValidationError::Invariant,
            )? {
                return invariant("CAS represented prefix is outside its selected path");
            }
            Ok(())
        }
    }
}

pub(super) fn prefix_contains(
    reader: &DomainReader<'_, SyndicDomain>,
    outer: crate::CasRepresentedPrefixProof,
    inner: crate::CasRepresentedPrefixProof,
) -> Result<bool, SyndicValidationError> {
    match (outer.tail(), inner.tail()) {
        (_, None) => Ok(inner.digest() == crate::empty_selected_path_digest()),
        (None, Some(_)) => Ok(false),
        (Some(outer_tail), Some(inner_tail)) => {
            let outer_turn =
                require::<TurnsFamily>(reader, &outer_tail, "outer CAS prefix is missing")?;
            let inner_turn =
                require::<TurnsFamily>(reader, &inner_tail, "inner CAS prefix is missing")?;
            if outer_turn.chain_digest() != outer.digest()
                || inner_turn.chain_digest() != inner.digest()
            {
                return invariant("CAS prefix digest disagrees with its tail");
            }
            crate::selected_path::includes_turn(
                outer_turn,
                &inner_turn,
                |turn_id| {
                    require::<TurnsFamily>(reader, &turn_id, "CAS prefix ancestor is missing")
                },
                SyndicValidationError::Invariant,
            )
        }
    }
}

pub(super) fn validate_usable(
    reader: &DomainReader<'_, SyndicDomain>,
    selected: crate::SelectedPathProof,
    usable: &crate::UsableCasBinding,
) -> Result<(), SyndicValidationError> {
    if usable.represented_prefix().tail().is_none()
        && usable.native_turn_count() != CasNativeTurnCount::ZERO
    {
        return invariant("empty CAS represented prefix has a nonzero native turn count");
    }
    validate_represented_prefix(reader, selected, usable.represented_prefix())?;
    let established = usable.lineage().established_prefix();
    validate_represented_prefix(reader, selected, established)?;
    if established.source_thread_revision() > usable.represented_prefix().source_thread_revision()
        || !prefix_contains(reader, usable.represented_prefix(), established)?
    {
        return invariant("CAS lineage establishment is not within its represented prefix");
    }
    Ok(())
}

pub(super) fn validate_stale(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    stale: &crate::StaleCasBinding,
) -> Result<(), SyndicValidationError> {
    if let Some(prefix) = stale.observed_prefix() {
        if prefix.tail().is_none()
            && stale
                .observed_native_turn_count()
                .is_some_and(|count| count != CasNativeTurnCount::ZERO)
        {
            return invariant("empty stale CAS prefix has a nonzero native turn count");
        }
        validate_represented_prefix(reader, binding.selected_path(), prefix)?;
    }
    if let Some(lineage) = stale.observed_lineage() {
        let established = lineage.established_prefix();
        validate_represented_prefix(reader, binding.selected_path(), established)?;
        if let Some(prefix) = stale.observed_prefix()
            && !prefix_contains(reader, prefix, established)?
        {
            return invariant("stale lineage establishment exceeds its observed prefix");
        }
        if let Some(injection_generation) = lineage.recovered_injection_generation()
            && stale.loaded_generation().is_none_or(|current_generation| {
                current_generation.process() != injection_generation.process()
            })
        {
            return invariant("stale recovered lineage process generation disagrees");
        }
    }
    Ok(())
}

pub(super) fn validate_pending_prefix(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    prefix: crate::CasRepresentedPrefixProof,
) -> Result<(), SyndicValidationError> {
    let Some(tail) = binding.selected_path().tail() else {
        return Ok(());
    };
    let turn = require::<TurnsFamily>(reader, &tail, "binding selected turn is missing")?;
    let state =
        require::<TurnStatesFamily>(reader, &tail, "binding selected turn state is missing")?;
    if !state.lifecycle().blocks_same_thread_start() {
        return Ok(());
    }
    let (expected_tail, expected_digest) = match turn.parent().turn() {
        Some(parent) => {
            let parent = require::<TurnsFamily>(reader, &parent, "pending turn parent is missing")?;
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, crate::empty_selected_path_digest()),
    };
    if prefix.tail() != expected_tail
        || prefix.digest() != expected_digest
        || prefix.source_thread_revision() != binding.selected_path().thread_revision()
    {
        return invariant("pending binding does not represent exactly its parent prefix");
    }
    Ok(())
}

pub(super) fn validate_current_usable_prefix(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    prefix: crate::CasRepresentedPrefixProof,
) -> Result<(), SyndicValidationError> {
    let Some(tail) = binding.selected_path().tail() else {
        return validate_exact_selected(binding, prefix);
    };
    let state = require::<TurnStatesFamily>(
        reader,
        &tail,
        "current valid binding selected turn state is missing",
    )?;
    if state.lifecycle() == TurnLifecycle::Pending {
        return validate_pending_prefix(reader, binding, prefix);
    }
    if !state.lifecycle().is_proven_terminal() {
        return invariant("current valid binding selected turn is still live or terminal-unknown");
    }
    validate_exact_selected(binding, prefix)
}

fn validate_exact_selected(
    binding: &crate::BindingRecord,
    prefix: crate::CasRepresentedPrefixProof,
) -> Result<(), SyndicValidationError> {
    if prefix.tail() != binding.selected_path().tail()
        || prefix.digest() != binding.selected_path().digest()
        || prefix.source_thread_revision() != binding.selected_path().thread_revision()
    {
        return invariant("current valid binding does not represent its exact selected path");
    }
    Ok(())
}
