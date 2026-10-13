use super::{model::DraftSealedImportBasisV1, read::SourceReader};
use crate::{
    CanonicalItemKind, DiscussionHandoffGateState, DraftSubmissionIntent, InputGateState,
    ProjectionLifecycle, SyndicReadError, ThreadArchiveState, TurnKind, codec::*,
};

pub(super) fn source_is_exact(
    r: &impl SourceReader,
    b: DraftSealedImportBasisV1,
) -> Result<bool, SyndicReadError> {
    macro_rules! get {
        ($family:ty,$key:expr) => {
            match r.point::<$family>(&$key)? {
                Some(v) => v,
                None => return Ok(false),
            }
        };
    }
    let thread = get!(ThreadsFamily, b.selector.thread_id());
    let draft = get!(DraftsFamily, b.selector.draft_id());
    let reverse = get!(DraftByThreadFamily, thread.id());
    let gate = get!(InputGatesFamily, thread.id());
    let attributes = get!(ThreadAttributesFamily, thread.id());
    let handoff = r.point::<DiscussionHandoffGatesFamily>(&thread.id())?;
    let editable = match (thread.parent_thread_id(), attributes.archive(), handoff) {
        (None, ThreadArchiveState::Ordinary, None) => thread.context_owner_id().is_none(),
        (Some(_), ThreadArchiveState::BranchDiscussionOpen, Some(g)) => {
            g.thread_id() == thread.id()
                && thread.context_owner_id().is_some()
                && g.state() == DiscussionHandoffGateState::Open
        }
        _ => false,
    };
    let path = b.intent.selected_path();
    let entry = b.intent.transcript_entry();
    if !editable
        || thread.id() != b.selector.thread_id()
        || gate.thread_id() != thread.id()
        || attributes.thread_id() != thread.id()
        || thread.revision() != b.selector.thread_revision()
        || thread.current_draft_id() != draft.id()
        || thread.id() != draft.thread_id()
        || draft.revision() != b.selector.selector_revision()
        || draft.root_history().root() != b.selector.root()
        || draft.history() != b.selector.history()
        || draft.submission_intent() != DraftSubmissionIntent::Ordinary
        || reverse.thread_id() != thread.id()
        || reverse.draft_id() != draft.id()
        || reverse.draft_revision() != draft.revision()
        || reverse.thread_revision() != thread.revision()
        || gate.revision() != b.gate_revision
        || gate.state() != &InputGateState::Idle
        || gate.live_count() != 0
        || path.tail() != thread.committed_tail()
        || path.digest() != thread.selected_path_digest()
        || path.thread_revision() != thread.revision()
    {
        return Ok(false);
    }
    let target = get!(TurnsFamily, b.intent.target_turn_id());
    let head = get!(TranscriptHeadsFamily, thread.id());
    let row = get!(
        TranscriptEntriesFamily,
        ThreadTranscriptKey {
            thread: thread.id(),
            generation: entry.generation(),
            position: entry.position()
        }
    );
    let item = get!(CanonicalItemsFamily, b.item_id);
    if let Some(origin_gate) = r.point::<InputGatesFamily>(&target.origin_thread_id())?
        && let InputGateState::RepairRequired(repair) = origin_gate.state()
        && repair.turn_id() == target.id()
    {
        return Ok(false);
    }
    Ok(target.id() == b.intent.target_turn_id()
        && item.id() == b.item_id
        && target.kind() == TurnKind::OrdinaryUser
        && head.lifecycle() == ProjectionLifecycle::Current
        && head.generation() == entry.generation()
        && head.committed_tail() == path.tail()
        && head.selected_path_digest() == path.digest()
        && row.thread_id() == thread.id()
        && row.generation() == entry.generation()
        && row.position() == entry.position()
        && row.item_id() == item.id()
        && row.item_revision() == item.revision()
        && item.revision() == b.item_revision
        && item.turn_id() == target.id()
        && item.kind() == CanonicalItemKind::UserInput
        && item.presentation()
            == &crate::CanonicalItemPresentation::user_input(b.content, b.assets))
}
