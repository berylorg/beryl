use super::*;

pub(super) fn validate_child_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<TurnChildrenFamily>(reader, |key, index| {
        if key.parent != index.parent_id() || key.child != index.child_id() {
            return invariant("turn-child index key disagrees");
        }
        let parent =
            require::<TurnsFamily>(reader, &key.parent, "turn-child index parent is missing")?;
        let child =
            require::<TurnsFamily>(reader, &key.child, "turn-child index child is missing")?;
        if child.parent() != ConversationParent::Turn(parent.id())
            || child.depth() != index.child_depth()
            || child.chain_digest() != index.child_digest()
        {
            return invariant("turn-child index is contradictory");
        }
        Ok(())
    })
}

pub(super) fn validate_replacement_intent(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    intent: ReplacementEditIntent,
) -> Result<(), SyndicValidationError> {
    let proof = intent.selected_path();
    if proof.tail() != thread.committed_tail()
        || proof.thread_revision() != thread.revision()
        || proof.digest() != thread.selected_path_digest()
    {
        return invariant("replacement edit selected-path proof disagrees with current thread");
    }
    let Some(tail) = proof.tail() else {
        return invariant("replacement edit target requires a selected path");
    };
    let head = require::<TranscriptHeadsFamily>(
        reader,
        &thread.id(),
        "replacement edit transcript head is missing",
    )?;
    if head.lifecycle() != ProjectionLifecycle::Current
        || head.generation() != intent.transcript_entry().generation()
        || head.committed_tail() != Some(tail)
        || head.selected_path_digest() != proof.digest()
    {
        return invariant("replacement edit transcript proof is not current");
    }
    let entry_key = ThreadTranscriptKey {
        thread: thread.id(),
        generation: intent.transcript_entry().generation(),
        position: intent.transcript_entry().position(),
    };
    let entry = require::<TranscriptEntriesFamily>(
        reader,
        &entry_key,
        "replacement edit transcript entry is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &entry.item_id(),
        "replacement edit transcript item is missing",
    )?;
    if entry.thread_id() != thread.id()
        || entry.generation() != intent.transcript_entry().generation()
        || entry.position() != intent.transcript_entry().position()
        || item.id() != entry.item_id()
        || item.revision() != entry.item_revision()
        || item.turn_id() != intent.target_turn_id()
        || item.kind() != CanonicalItemKind::UserInput
    {
        return invariant("replacement edit transcript entry or user item disagrees");
    }
    Ok(())
}
