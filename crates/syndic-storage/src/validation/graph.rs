use beryl_home_store::DomainReader;
use beryl_model::{DiscussionContextOwnerId, SyndicTurnId};

use crate::{
    CanonicalItemKind, ConversationParent, DraftByThreadRecord, DraftSubmissionIntent,
    ProjectionLifecycle, ReplacementEditIntent, ThreadParentIndexRecord, TurnChildIndexRecord,
    child_turn_chain_digest, codec::*, domain::SyndicDomain, empty_selected_path_digest,
    error::SyndicValidationError, root_thread_lineage_digest, root_turn_chain_digest,
};

use super::scan::{point, require, scan};

mod child;

use child::{validate_child_indexes, validate_replacement_intent};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_threads(reader)?;
    validate_drafts(reader)?;
    validate_draft_indexes(reader)?;
    validate_thread_parent_indexes(reader)?;
    validate_turns(reader)?;
    validate_turn_states(reader)?;
    validate_child_indexes(reader)
}

fn validate_threads(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |key, thread| {
        if *key != thread.id() {
            return invariant("thread key and record identity disagree");
        }
        let draft = require::<DraftsFamily>(
            reader,
            &thread.current_draft_id(),
            "thread current draft is missing",
        )?;
        if draft.thread_id() != thread.id() {
            return invariant("thread current draft has another owner");
        }
        let expected =
            DraftByThreadRecord::new(thread.id(), draft.id(), draft.revision(), thread.revision());
        if require::<DraftByThreadFamily>(
            reader,
            &thread.id(),
            "thread draft reverse index is missing",
        )? != expected
        {
            return invariant("thread draft reverse index disagrees");
        }
        match thread.committed_tail() {
            Some(tail) => {
                let turn =
                    require::<TurnsFamily>(reader, &tail, "thread committed tail is missing")?;
                if turn.chain_digest() != thread.selected_path_digest() {
                    return invariant("thread committed-tail digest disagrees");
                }
            }
            None if thread.selected_path_digest() == empty_selected_path_digest() => {}
            None => return invariant("empty thread has a noncanonical selected-path digest"),
        }
        match (thread.parent_thread_id(), thread.context_owner_id()) {
            (None, None) => {
                if thread.lineage_depth() != crate::ThreadLineageDepth::FIRST
                    || thread.lineage_ancestor_skip().is_some()
                    || thread.lineage_digest() != root_thread_lineage_digest(thread.id())
                    || thread.image_label_frontiers().inherited()
                        != crate::ImageLabelFrontier::EMPTY
                {
                    return invariant("top-level thread lineage or inherited-label facts disagree");
                }
            }
            (Some(parent), Some(owner)) => {
                let parent_record =
                    require::<ThreadsFamily>(reader, &parent, "thread parent is missing")?;
                if parent == thread.id() {
                    return invariant("thread parent is self-owned");
                }
                let parent_labels = parent_record.image_label_frontiers();
                let (depth, digest, skip) = crate::thread_lineage::child_shape(
                    thread.id(),
                    parent_record,
                    |id| require::<ThreadsFamily>(reader, &id, "thread ancestor is missing"),
                    SyndicValidationError::Invariant,
                )?;
                if thread.lineage_depth() != depth
                    || thread.lineage_digest() != digest
                    || thread.lineage_ancestor_skip() != Some(skip)
                    || thread.image_label_frontiers().inherited() > parent_labels.current()
                {
                    return invariant("child thread lineage or inherited-label facts disagree");
                }
                let envelope = require::<ContextEnvelopesFamily>(
                    reader,
                    &ContextOwnerKey::from(owner),
                    "thread context envelope is missing",
                )?;
                if envelope.owner() != owner
                    || envelope.envelope().descriptor().source_thread_id() != parent
                {
                    return invariant("thread context envelope source parent disagrees");
                }
                let key = ThreadPairKey {
                    first: parent,
                    second: thread.id(),
                };
                let expected =
                    ThreadParentIndexRecord::new(parent, thread.id(), thread.revision(), owner);
                if require::<ThreadParentFamily>(reader, &key, "thread parent index is missing")?
                    != expected
                {
                    return invariant("thread parent index disagrees");
                }
            }
            _ => return invariant("thread parent and context owner must appear together"),
        }
        Ok(())
    })
}

fn validate_drafts(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<DraftsFamily>(reader, |key, draft| {
        if *key != draft.id() {
            return invariant("draft key and identity disagree");
        }
        let thread =
            require::<ThreadsFamily>(reader, &draft.thread_id(), "draft owner thread is missing")?;
        if thread.current_draft_id() != draft.id() {
            return invariant("draft is not its thread's current draft");
        }
        if point::<TurnsFamily>(reader, &SyndicTurnId::from_bytes(*draft.id().as_bytes()))?
            .is_some()
        {
            return invariant("live draft and submitted turn reuse one raw identity");
        }
        if point::<AcceptedInputsFamily>(reader, &draft.id().accepted_input_id())?.is_some() {
            return invariant("live draft and accepted input reuse one raw identity");
        }
        if draft.updated_at() < draft.created_at() {
            return invariant("draft update timestamp precedes creation");
        }
        match draft.submission_intent() {
            DraftSubmissionIntent::Ordinary => {
                if matches!(
                    thread.context_owner_id(),
                    Some(DiscussionContextOwnerId::Draft(_))
                ) {
                    return invariant("ordinary draft owns thread context");
                }
            }
            DraftSubmissionIntent::DiscussionContext(DiscussionContextOwnerId::Draft(id))
                if id == draft.id()
                    && thread.context_owner_id() == Some(DiscussionContextOwnerId::Draft(id)) => {}
            DraftSubmissionIntent::DiscussionContext(_) => {
                return invariant("draft context owner disagrees with draft or thread");
            }
            DraftSubmissionIntent::Replacement(intent) => {
                let target = require::<TurnsFamily>(
                    reader,
                    &intent.target_turn_id(),
                    "draft replacement target is missing",
                )?;
                if target.kind() != crate::TurnKind::OrdinaryUser {
                    return invariant("replacement edit target is not an ordinary user turn");
                }
                validate_replacement_intent(reader, &thread, intent)?;
            }
        }
        if !matches!(draft.submission_intent(), DraftSubmissionIntent::Ordinary) {
            let gate =
                require::<InputGatesFamily>(reader, &thread.id(), "special draft gate is missing")?;
            if !matches!(gate.state(), crate::InputGateState::Idle) || gate.live_count() != 0 {
                return invariant("special draft has live or non-idle input authority");
            }
        }
        Ok(())
    })
}

fn validate_draft_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<DraftByThreadFamily>(reader, |key, index| {
        if *key != index.thread_id() {
            return invariant("draft reverse key disagrees");
        }
        let thread = require::<ThreadsFamily>(reader, key, "draft reverse index owner is missing")?;
        let draft = require::<DraftsFamily>(
            reader,
            &index.draft_id(),
            "draft reverse index target is missing",
        )?;
        let expected =
            DraftByThreadRecord::new(thread.id(), draft.id(), draft.revision(), thread.revision());
        if *index != expected
            || thread.current_draft_id() != draft.id()
            || draft.thread_id() != thread.id()
        {
            return invariant("draft reverse index is contradictory");
        }
        Ok(())
    })
}

fn validate_thread_parent_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ThreadParentFamily>(reader, |key, index| {
        if key.first != index.parent_thread_id() || key.second != index.child_thread_id() {
            return invariant("thread parent index key disagrees");
        }
        require::<ThreadsFamily>(
            reader,
            &index.parent_thread_id(),
            "thread parent index parent is missing",
        )?;
        let child = require::<ThreadsFamily>(
            reader,
            &index.child_thread_id(),
            "thread parent index child is missing",
        )?;
        if child.parent_thread_id() != Some(index.parent_thread_id())
            || child.context_owner_id() != Some(index.context_owner_id())
            || child.revision() != index.child_revision()
        {
            return invariant("thread parent index and child disagree");
        }
        Ok(())
    })
}

pub(super) fn validate_context_envelopes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ContextEnvelopesFamily>(reader, |key, record| {
        if DiscussionContextOwnerId::from(*key) != record.owner() || record.revision().get() != 1 {
            return invariant("context-envelope key, owner, or immutable revision disagrees");
        }
        let (child_thread, submitted_parent) = match record.owner() {
            DiscussionContextOwnerId::Draft(id) => {
                let draft =
                    require::<DraftsFamily>(reader, &id, "draft-owned context has no draft")?;
                let thread = require::<ThreadsFamily>(
                    reader,
                    &draft.thread_id(),
                    "context owner thread is missing",
                )?;
                if draft.submission_intent()
                    != DraftSubmissionIntent::DiscussionContext(record.owner())
                    || thread.context_owner_id() != Some(record.owner())
                {
                    return invariant("draft-owned context reverse agreement failed");
                }
                (thread, None)
            }
            DiscussionContextOwnerId::SubmittedTurn(id) => {
                let turn = require::<TurnsFamily>(reader, &id, "turn-owned context has no turn")?;
                let thread = require::<ThreadsFamily>(
                    reader,
                    &turn.origin_thread_id(),
                    "context turn origin is missing",
                )?;
                if thread.context_owner_id() != Some(record.owner()) {
                    return invariant("turn-owned context reverse agreement failed");
                }
                (thread, Some(turn.parent()))
            }
        };
        let source = record.envelope().descriptor().source();
        if child_thread.parent_thread_id() != Some(source.thread_id()) {
            return invariant("context child thread and source thread disagree");
        }
        if submitted_parent
            .is_some_and(|parent| parent != ConversationParent::Turn(source.turn_id()))
        {
            return invariant("context owner parent and source turn disagree");
        }
        require::<ThreadsFamily>(
            reader,
            &source.thread_id(),
            "context source thread is missing",
        )?;
        let item = require::<CanonicalItemsFamily>(
            reader,
            &source.item_id(),
            "context source item is missing",
        )?;
        if !matches!(item.kind(), CanonicalItemKind::AssistantMessage(_)) {
            return invariant("context source item is not an assistant message");
        }
        let projection = require::<ProjectionsFamily>(
            reader,
            &source.projection_id(),
            "context source projection is missing",
        )?;
        let source_state = require::<TurnStatesFamily>(
            reader,
            &source.turn_id(),
            "context source turn state is missing",
        )?;
        if !source_state.lifecycle().is_proven_terminal()
            || source_state.finalized_item_count() < item.ordinal().get()
        {
            return invariant("context source turn is not finalized terminal history");
        }
        if item.turn_id() != source.turn_id()
            || projection.item_id() != source.item_id()
            || projection.turn_id() != source.turn_id()
            || projection.revision() != source.projection_revision()
        {
            return invariant("context source records disagree");
        }
        let head = require::<ItemProjectionHeadsFamily>(
            reader,
            &item.id(),
            "context source item projection head is missing",
        )?;
        if head.lifecycle() != ProjectionLifecycle::Current
            || head.source_item_revision() != item.revision()
        {
            return invariant("context source item projection is not current");
        }
        let set = require::<ItemProjectionSetsFamily>(
            reader,
            &ItemProjectionSetKey {
                item: item.id(),
                generation: head.generation(),
            },
            "context source item projection set is missing",
        )?;
        let Some(index) = crate::membership::point(reader, &set, projection.ordinal())? else {
            return invariant("context source projection is outside its current item set");
        };
        if set.source_item_revision() != item.revision()
            || index.projection_id() != projection.id()
            || index.projection_revision() != projection.revision()
        {
            return invariant("context source current projection set disagrees");
        }
        let range = source.range();
        let Some(projection_range) = projection.payload().source_range() else {
            return invariant("context source projection has no selectable text range");
        };
        if range.start() < projection_range.start() || range.end() > projection_range.end() {
            return invariant("context range lies outside its source projection");
        }
        let bytes = super::read_projection_text_range(
            reader,
            item.projection_source()
                .ok_or(SyndicValidationError::Invariant(
                    "context source assistant item omitted projection text",
                ))?,
            range.start(),
            range.end(),
        )?;
        if bytes != record.envelope().text().as_str().as_bytes() {
            return invariant("context source range and exact text disagree");
        }
        Ok(())
    })
}

fn validate_turns(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<TurnsFamily>(reader, |key, turn| {
        if *key != turn.id() {
            return invariant("turn key and identity disagree");
        }
        if point::<AcceptedInputsFamily>(
            reader,
            &beryl_model::SyndicAcceptedInputId::from_bytes(*turn.id().as_bytes()),
        )?
        .is_some()
        {
            return invariant("submitted turn and accepted input reuse one raw identity");
        }
        require::<ThreadsFamily>(
            reader,
            &turn.origin_thread_id(),
            "turn origin thread is missing",
        )?;
        let state = require::<TurnStatesFamily>(reader, key, "turn state is missing")?;
        if state.turn_id() != turn.id() {
            return invariant("turn and turn-state identities disagree");
        }
        match turn.parent() {
            ConversationParent::Root => {
                if turn.depth().get() != 1
                    || turn.ancestor_skip().is_some()
                    || turn.chain_digest() != root_turn_chain_digest(turn.id())
                {
                    return invariant("root turn depth, ancestor skip, or chain digest is invalid");
                }
            }
            ConversationParent::Turn(parent_id) => {
                let parent = require::<TurnsFamily>(reader, &parent_id, "turn parent is missing")?;
                if parent.depth().get().checked_add(1) != Some(turn.depth().get())
                    || turn.chain_digest()
                        != child_turn_chain_digest(turn.id(), parent_id, parent.chain_digest())
                {
                    return invariant(
                        "child turn depth, ancestor skip, or chain digest is invalid",
                    );
                }
                let expected_skip = crate::selected_path::child_ancestor_skip(
                    parent.clone(),
                    turn.depth(),
                    |turn_id| {
                        require::<TurnsFamily>(
                            reader,
                            &turn_id,
                            "turn ancestor-skip target is missing",
                        )
                    },
                    SyndicValidationError::Invariant,
                )?;
                if turn.ancestor_skip() != Some(expected_skip) {
                    return invariant(
                        "child turn depth, ancestor skip, or chain digest is invalid",
                    );
                }
                let child_key = TurnPairKey {
                    parent: parent_id,
                    child: turn.id(),
                };
                let expected = TurnChildIndexRecord::new(
                    parent_id,
                    turn.id(),
                    turn.depth(),
                    turn.chain_digest(),
                );
                if require::<TurnChildrenFamily>(reader, &child_key, "turn child index is missing")?
                    != expected
                {
                    return invariant("turn child index disagrees");
                }
            }
        }
        Ok(())
    })
}

fn validate_turn_states(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<TurnStatesFamily>(reader, |key, state| {
        if *key != state.turn_id() {
            return invariant("turn state has no matching immutable turn");
        }
        let turn =
            require::<TurnsFamily>(reader, key, "turn state has no matching immutable turn")?;
        if state.lifecycle().blocks_same_thread_start() {
            let thread = require::<ThreadsFamily>(
                reader,
                &turn.origin_thread_id(),
                "blocking turn origin thread is missing",
            )?;
            let provider_operation = turn.kind()
                == crate::TurnKind::ProviderOperation(
                    crate::ProviderOperationKind::ContextCompaction,
                )
                && turn.parent() == crate::ConversationParent::Root;
            if provider_operation {
                let gate = require::<InputGatesFamily>(
                    reader,
                    &thread.id(),
                    "blocking provider turn input gate is missing",
                )?;
                let operation_id = crate::CompactionOperationId::new(
                    thread.id(),
                    crate::CompactionOperationNonce::from_bytes(*turn.id().as_bytes()),
                );
                let operation = require::<CompactionOperationsFamily>(
                    reader,
                    &operation_id,
                    "blocking provider turn compaction operation is missing",
                )?;
                let selected = match gate.state() {
                    crate::InputGateState::Compacting {
                        turn_id,
                        operation_nonce,
                    } => {
                        *turn_id == turn.id()
                            && *operation_nonce == operation_id.nonce()
                            && operation.state().is_live()
                    }
                    crate::InputGateState::Stopping {
                        turn_id,
                        operation_nonce,
                    } => {
                        let stop = require::<StopOperationsFamily>(
                            reader,
                            &crate::StopOperationId::new(thread.id(), *operation_nonce),
                            "blocking provider turn stop operation is missing",
                        )?;
                        *turn_id == turn.id()
                            && stop.state().is_live()
                            && stop.target().turn_id() == turn.id()
                            && operation.state()
                                == &crate::CompactionOperationState::Stopping(*operation_nonce)
                    }
                    _ => false,
                };
                if !selected {
                    return invariant("blocking provider turn authority disagrees");
                }
            } else if thread.committed_tail() != Some(turn.id()) {
                return invariant("blocking turn is not its origin thread's committed tail");
            }
        }
        Ok(())
    })
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
