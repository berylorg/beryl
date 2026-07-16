use beryl_home_store::DomainReader;

use crate::validation::scan::{point, require, scan};
use crate::{
    CanonicalItemKind, CanonicalItemPayload, CasItemIndexRecord, GeneratedMediaResourceDisposition,
    ProviderItemLifecycle, ResourceBacking, TurnItemIndexRecord, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::invariant;

pub(super) fn validate_items(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |key, item| {
        if *key != item.id() {
            return invariant("canonical-item key and identity disagree");
        }
        require::<TurnsFamily>(
            reader,
            &item.turn_id(),
            "canonical item owner turn is missing",
        )?;
        let index_key = TurnItemKey {
            owner: item.turn_id(),
            ordinal: item.ordinal(),
        };
        let expected =
            TurnItemIndexRecord::new(item.turn_id(), item.ordinal(), item.id(), item.revision());
        if require::<TurnItemsFamily>(reader, &index_key, "turn-item index is missing")? != expected
        {
            return invariant("turn-item index disagrees");
        }
        if let Some(sequence) = item.source_event() {
            let event = require::<SourceEventsFamily>(
                reader,
                &TurnEventKey {
                    owner: item.turn_id(),
                    ordinal: sequence,
                },
                "canonical item source event is missing",
            )?;
            if event.turn_id() != item.turn_id() {
                return invariant("canonical item source event has another owner");
            }
            if let (Some(event_source), Some(item_source)) = (event.source(), item.cas_source())
                && (event_source.thread_id() != item_source.turn().thread_id()
                    || event_source.turn_id() != item_source.turn().turn_id())
            {
                return invariant("canonical item and source event CAS provenance disagrees");
            }
        }
        if let Some(source) = item.cas_source() {
            validate_cas_turn_source(
                reader,
                item.turn_id(),
                source.turn().thread_id(),
                source.turn().turn_id(),
                "canonical item CAS-turn index is missing",
                "canonical item CAS-turn correlation disagrees",
            )?;
            let key = CasItemKey::Record(
                source.turn().thread_id().clone(),
                source.turn().turn_id().clone(),
                source.item_id().clone(),
            );
            let expected = CasItemIndexRecord::new(
                source.turn().thread_id().clone(),
                source.turn().turn_id().clone(),
                source.item_id().clone(),
                item.id(),
                item.revision(),
            );
            if require::<CasItemIndexFamily>(reader, &key, "CAS item reverse index is missing")?
                != expected
            {
                return invariant("CAS item reverse index disagrees");
            }
        }
        Ok(())
    })
}

pub(super) fn validate_turn_items(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_turn = None;
    let mut expected = 1_u64;
    let mut observed = 0_u64;
    let mut open_items = 0_u64;
    let mut history_blockers = 0_u64;
    scan::<TurnItemsFamily>(reader, |key, index| {
        if current_turn != Some(key.owner) {
            finish_turn_item_frontier(
                reader,
                current_turn,
                observed,
                open_items,
                history_blockers,
            )?;
            current_turn = Some(key.owner);
            expected = 1;
            observed = 0;
            open_items = 0;
            history_blockers = 0;
        }
        if key.owner != index.turn_id()
            || key.ordinal != index.ordinal()
            || index.ordinal().get() != expected
        {
            return invariant("turn-item key or contiguous order disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &index.item_id(),
            "turn-item target is missing",
        )?;
        if item.turn_id() != index.turn_id()
            || item.ordinal() != index.ordinal()
            || item.revision() != index.item_revision()
        {
            return invariant("turn-item target disagrees");
        }
        let state = require::<TurnStatesFamily>(
            reader,
            &index.turn_id(),
            "turn-item owner state is missing",
        )?;
        let content = item.payload().content();
        if let Some(content) = content {
            let manifest = require::<ContentManifestsFamily>(
                reader,
                &content.id(),
                "turn-item content manifest is missing",
            )?;
            if index.ordinal().get() <= state.finalized_item_count()
                && !manifest.lifecycle().is_immutable()
            {
                return invariant("finalized turn-item frontier contains live content");
            }
        }
        if index.ordinal().get() <= state.finalized_item_count()
            && let CanonicalItemPayload::GeneratedMedia(resource_id) = item.payload()
        {
            let resource = require::<ResourcesFamily>(
                reader,
                resource_id,
                "finalized generated item resource is missing",
            )?;
            if !matches!(
                resource.backing(),
                ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(_))
            ) {
                return invariant("finalized generated item has no owned asset authority");
            }
        }
        if index.ordinal().get() <= state.finalized_item_count()
            && matches!(
                item.kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            )
        {
            let content = content.ok_or(SyndicValidationError::Invariant(
                "finalized visible item omitted canonical content",
            ))?;
            let head = require::<ItemProjectionHeadsFamily>(
                reader,
                &item.id(),
                "finalized visible turn item has no projection head",
            )?;
            let set = require::<ItemProjectionSetsFamily>(
                reader,
                &ItemProjectionSetKey {
                    item: item.id(),
                    generation: head.generation(),
                },
                "finalized visible turn item has no projection set",
            )?;
            if head.lifecycle() != crate::ProjectionLifecycle::Current
                || head.source_item_revision() != item.revision()
                || set.source_item_revision() != item.revision()
                || set.source_content() != content
                || set.projection_count() == 0
            {
                return invariant("finalized visible turn item projection is not current");
            }
        }
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            open_items = open_items
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "turn open-item aggregate exhausted",
                ))?;
        }
        if item.disposition().is_history_blocking() {
            history_blockers =
                history_blockers
                    .checked_add(1)
                    .ok_or(SyndicValidationError::Invariant(
                        "turn history-blocker aggregate exhausted",
                    ))?;
        }
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "turn-item order exhausted",
            ))?;
        observed += 1;
        Ok(())
    })?;
    finish_turn_item_frontier(reader, current_turn, observed, open_items, history_blockers)?;
    scan::<TurnStatesFamily>(reader, |_, state| {
        if state.finalized_item_count() > state.item_count()
            || state.open_item_count() > state.item_count()
            || state.history_blocking_item_count() > state.item_count()
        {
            return invariant("turn finalized-item frontier exceeds its item frontier");
        }
        if state.lifecycle().is_proven_terminal()
            && (state.open_item_count() != 0 || state.history_blocking_item_count() != 0)
            && state.incomplete_reason().is_none()
        {
            return invariant("settled turn item audit is incomplete without a typed reason");
        }
        let key = TurnItemKey {
            owner: state.turn_id(),
            ordinal: crate::TurnItemOrdinal::FIRST,
        };
        if (state.item_count() == 0) == point::<TurnItemsFamily>(reader, &key)?.is_some() {
            return invariant("turn item zero frontier disagrees");
        }
        Ok(())
    })
}

fn finish_turn_item_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: Option<beryl_model::SyndicTurnId>,
    observed: u64,
    open_items: u64,
    history_blockers: u64,
) -> Result<(), SyndicValidationError> {
    let Some(turn) = turn else {
        return Ok(());
    };
    let state = require::<TurnStatesFamily>(reader, &turn, "turn-item owner state is missing")?;
    if state.item_count() != observed
        || state.open_item_count() != open_items
        || state.history_blocking_item_count() != history_blockers
    {
        return invariant("turn item frontier or capture aggregates disagree");
    }
    Ok(())
}

pub(super) fn validate_cas_items(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CasItemIndexFamily>(reader, |key, index| {
        let CasItemKey::Record(thread, turn, item_id) = key else {
            return invariant("stored CAS-item cursor sentinel");
        };
        if thread != index.cas_thread_id()
            || turn != index.cas_turn_id()
            || item_id != index.cas_item_id()
        {
            return invariant("CAS-item index key disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &index.item_id(),
            "CAS-item index target is missing",
        )?;
        let Some(source) = item.cas_source() else {
            return invariant("CAS-item target has no CAS provenance");
        };
        if source.turn().thread_id() != thread
            || source.turn().turn_id() != turn
            || source.item_id() != item_id
            || item.revision() != index.item_revision()
        {
            return invariant("CAS-item index target disagrees");
        }
        validate_cas_turn_source(
            reader,
            item.turn_id(),
            thread,
            turn,
            "CAS-item source CAS-turn index is missing",
            "CAS-item source CAS-turn correlation disagrees",
        )?;
        Ok(())
    })
}

pub(super) fn validate_cas_turn_source(
    reader: &DomainReader<'_, SyndicDomain>,
    syndic_turn: beryl_model::SyndicTurnId,
    cas_thread: &beryl_model::CasThreadId,
    cas_turn: &beryl_model::CasTurnId,
    missing: &'static str,
    mismatch: &'static str,
) -> Result<(), SyndicValidationError> {
    let index = require::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(cas_thread.clone(), cas_turn.clone()),
        missing,
    )?;
    if index.cas_thread_id() != cas_thread
        || index.cas_turn_id() != cas_turn
        || index.turn_id() != syndic_turn
    {
        return invariant(mismatch);
    }
    Ok(())
}
