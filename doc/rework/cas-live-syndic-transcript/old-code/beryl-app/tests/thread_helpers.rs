#![allow(dead_code, private_interfaces, unused_imports)]

use std::path::PathBuf;

use beryl_backend::{ThreadInfo, ThreadSummary};
use beryl_model::{
    conversation::{ConversationThreadId, ConversationTurnId, RegisteredConversationThread},
    workspace::WorkspaceId,
};
use serde_json::{Value, json};

mod shell {
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/thread_helpers.rs"]
    mod thread_helpers;

    use beryl_backend::{ThreadInfo, ThreadSummary};
    use beryl_model::conversation::RegisteredConversationThread;
    use beryl_model::workspace::WorkspaceId;

    pub(super) struct ConversationSurfaceState {
        execution_details: execution_detail::ExecutionDetailState,
    }

    impl ConversationSurfaceState {
        pub(super) fn from_thread(thread: &ThreadInfo) -> Self {
            let mut execution_details = execution_detail::ExecutionDetailState::default();
            execution_details.load_thread_history(thread);
            Self { execution_details }
        }
    }

    pub(super) fn first_real_branch_user_input_fragment_text<'a>(
        surface: &'a ConversationSurfaceState,
        thread: &RegisteredConversationThread,
    ) -> Option<&'a str> {
        thread_helpers::first_real_branch_user_input_fragment_text(surface, thread)
    }

    pub(super) fn registered_thread_from_summary(
        execution_target: &WorkspaceId,
        summary: &ThreadSummary,
    ) -> RegisteredConversationThread {
        thread_helpers::registered_thread_from_summary(execution_target, summary)
    }
}

use shell::{
    ConversationSurfaceState, first_real_branch_user_input_fragment_text,
    registered_thread_from_summary,
};

#[test]
fn branch_with_only_bootstrap_history_has_no_real_user_title_candidate() {
    let surface = ConversationSurfaceState::from_thread(&thread_info(vec![user_turn(
        "bootstrap_turn",
        "Branched from [Parent](beryl_threadid://parent), no response required.",
    )]));
    let branch = branch_registration();

    assert_eq!(
        first_real_branch_user_input_fragment_text(&surface, &branch),
        None
    );
}

#[test]
fn branch_title_candidate_skips_visible_bootstrap_and_uses_first_real_user_turn() {
    let surface = ConversationSurfaceState::from_thread(&thread_info(vec![
        user_turn(
            "bootstrap_turn",
            "Branched from [Parent](beryl_threadid://parent), no response required.",
        ),
        user_turn("real_turn_1", "First real branch question"),
        user_turn("real_turn_2", "Later branch follow-up"),
    ]));
    let branch = branch_registration();

    assert_eq!(
        first_real_branch_user_input_fragment_text(&surface, &branch),
        Some("First real branch question")
    );
}

#[test]
fn registered_thread_from_summary_preserves_valid_fork_parent_metadata() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\alpha");
    let parent_id = ConversationThreadId::new("parent_thread");
    let thread = registered_thread_from_summary(
        &execution_target,
        &thread_summary("branch_thread", Some(parent_id.as_str())),
    );

    assert_eq!(thread.branch_parent_thread_id(), Some(&parent_id));

    let self_parent = registered_thread_from_summary(
        &execution_target,
        &thread_summary("branch_thread", Some("branch_thread")),
    );
    assert_eq!(self_parent.branch_parent_thread_id(), None);
}

fn branch_registration() -> RegisteredConversationThread {
    RegisteredConversationThread::new(
        ConversationThreadId::new("branch_thread"),
        WorkspaceId::host_windows(r"C:\work\alpha"),
        "Branch preview",
        None,
        10,
        20,
    )
    .with_beryl_created()
    .with_transcript_branch_bootstrap(
        ConversationTurnId::new("source_turn"),
        Some(ConversationTurnId::new("bootstrap_turn")),
    )
}

fn thread_info(turns: Vec<Value>) -> ThreadInfo {
    serde_json::from_value(json!({
        "cliVersion": "0.128.0",
        "createdAt": 10,
        "cwd": r"C:\work\alpha",
        "ephemeral": false,
        "id": "branch_thread",
        "modelProvider": "openai",
        "preview": "Branch preview",
        "source": "appServer",
        "status": { "type": "idle" },
        "turns": turns,
        "updatedAt": 20
    }))
    .unwrap()
}

fn thread_summary(id: &str, forked_from_id: Option<&str>) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: forked_from_id.map(str::to_string),
        cwd: PathBuf::from(r"C:\work\alpha"),
        preview: "Branch preview".to_string(),
        name: Some("Branch".to_string()),
        agent_nickname: None,
        path: None,
        created_at: 10,
        updated_at: 20,
        model_provider: "openai".to_string(),
        ephemeral: false,
    }
}

fn user_turn(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "status": "completed",
        "items": [{
            "type": "userMessage",
            "id": format!("user_message_{id}"),
            "content": [{
                "type": "text",
                "text": text
            }]
        }]
    })
}
