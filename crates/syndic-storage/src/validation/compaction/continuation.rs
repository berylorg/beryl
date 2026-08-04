use beryl_home_store::DomainReader;

use crate::{
    BindingState, ConversationParent, TurnKind, TurnLifecycle, TurnStateRecord, codec::*,
    domain::SyndicDomain, error::SyndicValidationError,
};

use super::{invariant, require};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    operation: &crate::CompactionOperationRecord,
    receipt: &crate::CompactionSettlementReceiptRecord,
    turn_id: beryl_model::SyndicTurnId,
    item_id: beryl_model::SyndicItemId,
    content_id: beryl_model::SyndicContentId,
) -> Result<(), SyndicValidationError> {
    let continuation = receipt
        .continuation()
        .ok_or(SyndicValidationError::Invariant(
            "compaction continuation settlement receipt is incomplete",
        ))?;
    let prepared = crate::prepare_lifecycle_continuation_content()
        .map_err(|_| SyndicValidationError::Invariant("fixed continuation content is invalid"))?;
    let expected_turn = crate::derive_lifecycle_continuation_turn_id(
        operation.home_id(),
        operation.id(),
        prepared.summary().digest(),
    );
    let expected_item = crate::derive_lifecycle_continuation_item_id(
        operation.home_id(),
        operation.id(),
        prepared.summary().digest(),
    );
    let turn = require::<TurnsFamily>(
        reader,
        &turn_id,
        "compaction continuation successor turn is missing",
    )?;
    let continuation_state = require::<TurnStatesFamily>(
        reader,
        &turn_id,
        "compaction continuation successor state is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &item_id,
        "compaction continuation successor item is missing",
    )?;
    let content = require::<ContentManifestsFamily>(
        reader,
        &content_id,
        "compaction continuation successor content is missing",
    )?;
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &operation.target().snapshot_id(),
        "compaction continuation admission snapshot is missing",
    )?;
    let initial_binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: operation.target().thread_id(),
            revision: continuation.binding_revision(),
        },
        "compaction continuation initial binding is missing",
    )?;
    let expected_binding_revision = operation
        .target()
        .binding_revision()
        .checked_next()
        .map_err(|_| SyndicValidationError::Invariant("continuation binding revision exhausted"))?;
    let admission_path = snapshot.selected_path();
    let expected_parent = ConversationParent::from_turn(admission_path.tail());
    let (expected_depth, expected_digest, expected_ancestor_skip) =
        turn_shape(reader, turn_id, expected_parent)?;
    let expected_selected_path = crate::SelectedPathProof::new(
        Some(turn_id),
        admission_path
            .thread_revision()
            .checked_next()
            .map_err(|_| {
                SyndicValidationError::Invariant("continuation thread revision exhausted")
            })?,
        expected_digest,
    );
    if turn_id != expected_turn
        || item_id != expected_item
        || content_id != prepared.id()
        || continuation.content().id() != prepared.id()
        || continuation.content().encoding() != prepared.encoding()
        || continuation.content().summary() != prepared.summary()
        || turn.origin_thread_id() != operation.target().thread_id()
        || turn.kind() != TurnKind::BerylLifecycleContinuation
        || continuation.parent() != expected_parent
        || turn.parent() != expected_parent
        || turn.depth() != expected_depth
        || turn.chain_digest() != expected_digest
        || turn.ancestor_skip() != expected_ancestor_skip
        || continuation.selected_path() != expected_selected_path
        || continuation.binding_revision() != expected_binding_revision
        || initial_binding.thread_id() != operation.target().thread_id()
        || initial_binding.selected_path() != continuation.selected_path()
        || !matches!(initial_binding.state(), BindingState::Unbound { .. })
        || item.turn_id() != turn_id
        || item.ordinal() != crate::TurnItemOrdinal::FIRST
        || item.kind() != crate::CanonicalItemKind::UserInput
        || item.presentation_content() != Some(continuation.content())
        || item.presentation().asset_reference_set().is_some()
        || content.owner().is_some()
        || content.lifecycle() != crate::ContentLifecycle::Sealed
        || content.sealed_reference() != Some(continuation.content())
        || !lifecycle_is_descendant(&continuation_state)
    {
        return invariant("compaction continuation settlement and successor disagree");
    }
    Ok(())
}

fn turn_shape(
    reader: &DomainReader<'_, SyndicDomain>,
    turn_id: beryl_model::SyndicTurnId,
    parent: ConversationParent,
) -> Result<
    (
        crate::TurnDepth,
        beryl_model::SyndicPathDigest,
        Option<beryl_model::SyndicTurnId>,
    ),
    SyndicValidationError,
> {
    match parent {
        ConversationParent::Root => Ok((
            crate::TurnDepth::FIRST,
            crate::root_turn_chain_digest(turn_id),
            None,
        )),
        ConversationParent::Turn(parent_id) => {
            let parent = require::<TurnsFamily>(
                reader,
                &parent_id,
                "compaction continuation admission parent is missing",
            )?;
            let depth = parent.depth().checked_next().map_err(|_| {
                SyndicValidationError::Invariant("continuation turn depth exhausted")
            })?;
            let ancestor_skip = crate::selected_path::child_ancestor_skip(
                parent.clone(),
                depth,
                |turn_id| {
                    require::<TurnsFamily>(
                        reader,
                        &turn_id,
                        "compaction continuation admission ancestor is missing",
                    )
                },
                SyndicValidationError::Invariant,
            )?;
            Ok((
                depth,
                crate::child_turn_chain_digest(turn_id, parent_id, parent.chain_digest()),
                Some(ancestor_skip),
            ))
        }
    }
}

fn lifecycle_is_descendant(state: &TurnStateRecord) -> bool {
    if state.item_count() == 0 {
        return false;
    }
    match state.lifecycle() {
        TurnLifecycle::Pending => {
            state.revision() == crate::TurnStateRevision::FIRST
                && state.source_event_count() == 0
                && state.item_count() == 1
                && state.finalized_item_count() == 0
                && state.open_item_count() == 1
                && state.history_blocking_item_count() == 0
                && state.end_status().is_none()
        }
        TurnLifecycle::Active
        | TurnLifecycle::UnknownTerminal
        | TurnLifecycle::Complete
        | TurnLifecycle::Interrupted
        | TurnLifecycle::Failed
        | TurnLifecycle::Incomplete => {
            state.revision() > crate::TurnStateRevision::FIRST && state.source_event_count() > 0
        }
    }
}
