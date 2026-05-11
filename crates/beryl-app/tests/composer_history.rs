#![allow(dead_code)]

#[path = "../src/shell/composer_draft.rs"]
mod composer_draft;
#[path = "../src/shell/composer_history.rs"]
mod composer_history;

use composer_draft::{AcceptedComposerDraft, ComposerDraft, ComposerDraftImageData};
use composer_history::{ComposerHistoryBrowseResult, ComposerHistoryScope, ComposerHistoryState};
use gpui::ImageFormat;

fn accepted_text(text: &str) -> AcceptedComposerDraft {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(text.to_string());
    draft
        .accepted()
        .expect("text should produce accepted draft")
}

fn draft_text(text: &str) -> ComposerDraft {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(text.to_string());
    draft
}

fn accepted_image(text_prefix: &str, label: &str, bytes: &[u8]) -> AcceptedComposerDraft {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(text_prefix.to_string());
    draft.replace_range_with_image(
        text_prefix.len()..text_prefix.len(),
        label,
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), "asset"),
    );
    draft
        .accepted()
        .expect("image should produce accepted draft")
}

fn draft_image(text_prefix: &str, label: &str, bytes: &[u8]) -> ComposerDraft {
    let mut draft = ComposerDraft::default();
    draft.sync_display_text(text_prefix.to_string());
    draft.replace_range_with_image(
        text_prefix.len()..text_prefix.len(),
        label,
        ComposerDraftImageData::with_asset_id(ImageFormat::Png, bytes.to_vec(), "asset"),
    );
    draft
}

fn result_text(result: Option<ComposerHistoryBrowseResult>) -> Option<String> {
    match result {
        Some(ComposerHistoryBrowseResult::Accepted(draft)) => {
            Some(draft.display_text().to_string())
        }
        Some(ComposerHistoryBrowseResult::Draft(draft)) => Some(draft.display_text().to_string()),
        None => None,
    }
}

#[test]
fn empty_history_browse_is_noop() {
    let mut history = ComposerHistoryState::with_capacity(4);
    let scope = ComposerHistoryScope::Thread("thread-a".to_string());

    assert_eq!(
        history.browse_previous(scope.clone(), draft_text("current")),
        None
    );
    assert_eq!(history.browse_next(scope), None);
}

#[test]
fn previous_captures_current_draft_and_next_restores_it_after_newest_entry() {
    let mut history = ComposerHistoryState::with_capacity(4);
    let scope = ComposerHistoryScope::Thread("thread-a".to_string());
    history.record_accepted(scope.clone(), accepted_text("first"));
    history.record_accepted(scope.clone(), accepted_text("second"));

    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text("unsent draft"))),
        Some("second".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text("ignored edit"))),
        Some("first".to_string())
    );
    assert_eq!(
        history.browse_previous(scope.clone(), draft_text("at oldest")),
        None
    );
    assert_eq!(
        result_text(history.browse_next(scope.clone())),
        Some("second".to_string())
    );
    assert_eq!(
        result_text(history.browse_next(scope.clone())),
        Some("unsent draft".to_string())
    );
    assert_eq!(history.browse_next(scope), None);
}

#[test]
fn consecutive_duplicates_collapse_without_collapsing_non_adjacent_entries() {
    let mut history = ComposerHistoryState::with_capacity(8);
    let scope = ComposerHistoryScope::Thread("thread-a".to_string());
    history.record_accepted(scope.clone(), accepted_text("same"));
    history.record_accepted(scope.clone(), accepted_text("same"));
    history.record_accepted(scope.clone(), accepted_text("other"));
    history.record_accepted(scope.clone(), accepted_text("same"));

    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("same".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("other".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("same".to_string())
    );
    assert_eq!(history.browse_previous(scope, draft_text("")), None);
}

#[test]
fn bounded_history_evicts_oldest_entries() {
    let mut history = ComposerHistoryState::with_capacity(3);
    let scope = ComposerHistoryScope::Thread("thread-a".to_string());
    for index in 0..5 {
        history.record_accepted(scope.clone(), accepted_text(&format!("entry {index}")));
    }

    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("entry 4".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("entry 3".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(scope.clone(), draft_text(""))),
        Some("entry 2".to_string())
    );
    assert_eq!(history.browse_previous(scope, draft_text("")), None);
}

#[test]
fn scopes_are_independent_between_threads_and_pending_new_thread() {
    let mut history = ComposerHistoryState::with_capacity(4);
    let thread_a = ComposerHistoryScope::Thread("thread-a".to_string());
    let thread_b = ComposerHistoryScope::Thread("thread-b".to_string());
    let pending = ComposerHistoryScope::PendingNewThread(7);
    history.record_accepted(thread_a.clone(), accepted_text("a"));
    history.record_accepted(thread_b.clone(), accepted_text("b"));
    history.record_accepted(pending.clone(), accepted_text("pending"));

    assert_eq!(
        result_text(history.browse_previous(thread_a, draft_text(""))),
        Some("a".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(thread_b, draft_text(""))),
        Some("b".to_string())
    );
    assert_eq!(
        result_text(history.browse_previous(pending, draft_text(""))),
        Some("pending".to_string())
    );
}

#[test]
fn pending_new_thread_history_promotes_to_created_thread() {
    let mut history = ComposerHistoryState::with_capacity(4);
    let pending = ComposerHistoryScope::PendingNewThread(7);
    let thread = ComposerHistoryScope::Thread("created-thread".to_string());
    history.record_accepted(pending.clone(), accepted_text("first prompt"));

    history.bind_pending_new_thread_to_thread(7, "created-thread");

    assert_eq!(history.browse_previous(pending, draft_text("")), None);
    assert_eq!(
        result_text(history.browse_previous(thread, draft_text(""))),
        Some("first prompt".to_string())
    );
}

#[test]
fn image_entries_and_original_image_drafts_are_preserved() {
    let mut history = ComposerHistoryState::with_capacity(4);
    let scope = ComposerHistoryScope::Thread("thread-a".to_string());
    let accepted = accepted_image("see ", "A", b"accepted");
    let current = draft_image("draft ", "B", b"current");
    history.record_accepted(scope.clone(), accepted.clone());

    assert_eq!(
        history.browse_previous(scope.clone(), current.clone()),
        Some(ComposerHistoryBrowseResult::Accepted(accepted))
    );
    assert_eq!(
        history.browse_next(scope),
        Some(ComposerHistoryBrowseResult::Draft(current))
    );
}
