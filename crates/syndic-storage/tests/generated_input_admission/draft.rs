#[path = "../draft_edit_history_retention/common.rs"]
mod common;
#[path = "../draft_edit_history/support.rs"]
mod support;
use support::*;

pub(super) fn edit(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> DraftEditorCandidateSessionV1 {
    let current = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(400_000).unwrap())
        .unwrap()
        .unwrap();
    let session = open_session(storage, store, &current, 180, 181);
    common::commit_edit(storage, store, &session, 182, "unfinished parent draft")
        .adopted_session()
        .clone()
}
