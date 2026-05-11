use std::path::PathBuf;

use beryl_backend::ThreadSummary;
use beryl_model::workspace::WorkspaceId;

#[path = "../src/shell/thread_selection.rs"]
mod thread_selection;

use thread_selection::{
    KnownThreadSelection, ThreadSelectionRequest, resolve_known_thread_selection,
};

#[test]
fn exact_thread_selection_is_not_resolved_from_known_inventory() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let known_threads = vec![sample_thread("thread_a"), sample_thread("thread_b")];

    let selection = resolve_known_thread_selection(
        &known_threads,
        &execution_target,
        &ThreadSelectionRequest::exact("thread_b", "Release review"),
    );

    assert_eq!(selection, KnownThreadSelection::None);
}

#[test]
fn preferred_thread_selection_can_fall_back_to_the_first_known_thread() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let known_threads = vec![sample_thread("thread_a"), sample_thread("thread_b")];

    let selection = resolve_known_thread_selection(
        &known_threads,
        &execution_target,
        &ThreadSelectionRequest::RestorePreferred(Some("missing_thread".to_string())),
    );

    assert_eq!(
        selection,
        KnownThreadSelection::Selected {
            thread_id: "thread_a".to_string(),
            strict: false,
        }
    );
}

fn sample_thread(id: &str) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: None,
        cwd: PathBuf::from(r"C:\work\beryl"),
        preview: format!("Preview for {id}"),
        name: Some(format!("Thread {id}")),
        agent_nickname: None,
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}
