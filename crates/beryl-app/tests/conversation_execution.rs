#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;

use beryl_backend::{
    AgentMessageItem, CommandExecutionItem, CommandExecutionStatus, ImageGenerationItem,
    ProtocolPhase, ThreadItem, ThreadSessionResponse, TurnInfo, TurnStatus, TurnStreamEvent,
};
use execution_detail::{
    ExecutionDetailState, ExecutionItem, LastTurnState, MAX_COMMAND_OUTPUT_BYTES,
    MAX_ERROR_MESSAGE_BYTES, MAX_FILE_CHANGE_OUTPUT_BYTES, MAX_INLINE_GENERATED_IMAGE_RESULT_BYTES,
    MAX_REASONING_CONTENT_BYTES, TranscriptImagePathResolver, TranscriptImagePreviewState,
    TranscriptImageSourceResolution, TurnExecutionRecord, TurnExecutionStatus, TurnNarrativeEntry,
};
use serde_json::json;

#[test]
fn execution_detail_tracks_streamed_items_and_identifies_terminal_answer() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("List the workspace files".to_string());

    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::ItemStarted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::CommandExecution(CommandExecutionItem {
            id: "cmd_1".to_string(),
            command: "dir".to_string(),
            cwd: "C:\\work\\beryl".to_string(),
            status: CommandExecutionStatus::InProgress,
            process_id: None,
            aggregated_output: None,
            exit_code: None,
            duration_ms: None,
        }),
    });
    state.apply_stream_event(TurnStreamEvent::CommandExecutionOutputDelta {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item_id: "cmd_1".to_string(),
        delta: "Cargo.toml\n".to_string(),
    });
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::CommandExecution(CommandExecutionItem {
            id: "cmd_1".to_string(),
            command: "dir".to_string(),
            cwd: "C:\\work\\beryl".to_string(),
            status: CommandExecutionStatus::Completed,
            process_id: None,
            aggregated_output: Some("Cargo.toml\nsrc\n".to_string()),
            exit_code: Some(0),
            duration_ms: Some(12),
        }),
    });
    state.apply_stream_event(TurnStreamEvent::ItemStarted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "msg_1".to_string(),
            phase: Some(ProtocolPhase::Commentary),
            text: String::new(),
        }),
    });
    state.apply_stream_event(TurnStreamEvent::AgentMessageDelta {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item_id: "msg_1".to_string(),
        delta: "Checking the workspace.".to_string(),
    });
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "msg_2".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "The workspace contains Cargo.toml and src.".to_string(),
        }),
    });
    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    });

    let turn = &state.turns()[0];
    assert_eq!(turn.status, TurnExecutionStatus::Completed);
    assert_eq!(turn.thread_id.as_deref(), Some("thread_1"));
    assert_eq!(turn.turn_id.as_deref(), Some("turn_1"));
    assert_eq!(turn.terminal_assistant_item_id.as_deref(), Some("msg_2"));

    let command = turn
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::CommandExecution(item) => Some(item),
            _ => None,
        })
        .unwrap();
    assert_eq!(command.output, "Cargo.toml\nsrc\n");
    assert_eq!(command.exit_code, Some(0));

    let commentary = turn
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::AgentMessage(item) if item.phase == Some(ProtocolPhase::Commentary) => {
                Some(item)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(commentary.text, "Checking the workspace.");
}

#[test]
fn execution_detail_retained_counts_include_loaded_turn_payloads() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Inspect".to_string());
    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "msg_1".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "Done".to_string(),
        }),
    });

    let counts = state.retained_counts();

    assert_eq!(counts.turns, 1);
    assert_eq!(counts.items, 1);
    assert_eq!(counts.user_fragments, 1);
    assert_eq!(counts.user_fragment_text_bytes, "Inspect".len());
    assert_eq!(counts.backend_input_records, 1);
    assert_eq!(counts.backend_input_bytes, "Inspect".len());
    assert_eq!(counts.narrative_entries, 2);
    assert_eq!(counts.text_bytes, "Inspect".len() + "Done".len());
    assert_eq!(counts.agent_text_bytes, "Done".len());
    assert_eq!(counts.active_turn_payload_bytes, counts.payload_bytes);
    assert!(counts.identity_bytes >= "thread_1".len() + "turn_1".len() + "msg_1".len());
    assert!(counts.payload_bytes >= counts.text_bytes);
}

#[test]
fn execution_detail_keeps_failed_turn_without_fabricated_answer() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Run a broken command".to_string());

    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_2".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_2".to_string(),
            status: TurnStatus::Failed,
            items: Vec::new(),
            error: Some(beryl_backend::TurnError {
                message: "command failed".to_string(),
                additional_details: Some("exit code 1".to_string()),
            }),
        },
    });

    let turn = &state.turns()[0];
    assert_eq!(turn.status, TurnExecutionStatus::Failed);
    assert_eq!(
        turn.error_message.as_deref(),
        Some("command failed\n\nexit code 1")
    );
    assert_eq!(turn.terminal_assistant_item_id, None);
    assert!(turn.items.is_empty());
}

#[test]
fn execution_detail_bounds_operational_stream_payloads() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Run noisy work".to_string());
    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_noisy".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });

    state.apply_stream_event(TurnStreamEvent::CommandExecutionOutputDelta {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_noisy".to_string(),
        item_id: "cmd_noisy".to_string(),
        delta: "C".repeat(MAX_COMMAND_OUTPUT_BYTES + 1024),
    });
    state.apply_stream_event(TurnStreamEvent::FileChangeOutputDelta {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_noisy".to_string(),
        item_id: "file_noisy".to_string(),
        delta: "F".repeat(MAX_FILE_CHANGE_OUTPUT_BYTES + 1024),
    });
    state.apply_stream_event(TurnStreamEvent::ReasoningTextDelta {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_noisy".to_string(),
        item_id: "reason_noisy".to_string(),
        content_index: 0,
        delta: "R".repeat(MAX_REASONING_CONTENT_BYTES + 1024),
    });

    let turn = &state.turns()[0];
    let command = turn
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::CommandExecution(item) => Some(item),
            _ => None,
        })
        .expect("command output item should exist");
    assert!(command.output.len() <= MAX_COMMAND_OUTPUT_BYTES);
    assert!(
        command
            .output
            .contains("Beryl omitted additional command output")
    );

    let file_change = turn
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::FileChange(item) => Some(item),
            _ => None,
        })
        .expect("file-change output item should exist");
    assert!(file_change.output.len() <= MAX_FILE_CHANGE_OUTPUT_BYTES);
    assert!(
        file_change
            .output
            .contains("Beryl omitted additional file-change output")
    );

    let reasoning = turn
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::Reasoning(item) => Some(item),
            _ => None,
        })
        .expect("reasoning item should exist");
    assert!(reasoning.content[0].len() <= MAX_REASONING_CONTENT_BYTES);
    assert!(reasoning.content[0].contains("Beryl omitted additional reasoning detail"));

    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_noisy".to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    });

    let reasoning = state.turns()[0]
        .items
        .iter()
        .find_map(|item| match item {
            ExecutionItem::Reasoning(item) => Some(item),
            _ => None,
        })
        .expect("reasoning item should still exist");
    assert!(reasoning.content.is_empty());
}

#[test]
fn execution_detail_bounds_turn_error_detail() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Fail loudly".to_string());
    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_failed".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });

    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_failed".to_string(),
            status: TurnStatus::Failed,
            items: Vec::new(),
            error: Some(beryl_backend::TurnError {
                message: "failed".to_string(),
                additional_details: Some("E".repeat(MAX_ERROR_MESSAGE_BYTES + 1024)),
            }),
        },
    });

    let error = state.turns()[0]
        .error_message
        .as_deref()
        .expect("failed turn should retain bounded error detail");
    assert!(error.len() <= MAX_ERROR_MESSAGE_BYTES);
    assert!(error.contains("Beryl omitted additional turn error detail"));
}

#[test]
fn pending_turn_fragments_are_visible_but_not_stream_active_until_drained() {
    let mut state = ExecutionDetailState::default();
    let turn_index =
        state.begin_pending_turn_with_fragments(vec![execution_detail::UserInputFragment::text(
            "First queued prompt",
        )]);

    assert_eq!(turn_index, 0);
    assert_eq!(state.working_turn_index(), None);
    assert_eq!(state.last_turn_state(), LastTurnState::Unknown);
    assert_eq!(state.turns()[0].status, TurnExecutionStatus::Queued);

    state.append_user_input_fragment(
        turn_index,
        execution_detail::UserInputFragment::text("Second queued prompt"),
    );
    assert_eq!(
        state.turns()[0]
            .user_input_fragments()
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<Vec<_>>(),
        vec!["First queued prompt", "Second queued prompt"]
    );

    assert_eq!(
        state.apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "thread_1".to_string(),
            turn: TurnInfo {
                id: "turn_compact".to_string(),
                status: TurnStatus::InProgress,
                items: Vec::new(),
                error: None,
            },
        }),
        None
    );
    assert_eq!(state.turns()[0].thread_id.as_deref(), None);
    assert_eq!(state.turns()[0].turn_id.as_deref(), None);
    assert_eq!(state.turns()[0].status, TurnExecutionStatus::Queued);

    assert!(state.activate_pending_turn(turn_index));
    assert_eq!(state.working_turn_index(), Some(turn_index));
    assert_eq!(state.turns()[0].status, TurnExecutionStatus::Starting);
}

#[test]
fn active_turn_identity_is_unknown_until_turn_started_then_cleared_on_completion() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Initial prompt".to_string());

    let active = state.active_turn_identity().unwrap();
    assert_eq!(active.turn_index, 0);
    assert_eq!(active.thread_id, None);
    assert_eq!(active.turn_id, None);

    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });

    let active = state.active_turn_identity().unwrap();
    assert_eq!(active.turn_index, 0);
    assert_eq!(active.thread_id.as_deref(), Some("thread_1"));
    assert_eq!(active.turn_id.as_deref(), Some("turn_1"));

    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    });
    assert_eq!(state.active_turn_identity(), None);
}

#[test]
fn active_turn_steering_fragment_keeps_accepted_narrative_position() {
    let mut state = ExecutionDetailState::default();
    let turn_index = state.begin_turn("Initial prompt".to_string());

    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "assistant_before".to_string(),
            phase: Some(ProtocolPhase::Commentary),
            text: "Already rendered assistant output.".to_string(),
        }),
    });

    state.append_user_input_fragment(
        turn_index,
        execution_detail::UserInputFragment::text("Steered follow-up"),
    );
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "assistant_after".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "Assistant after steering.".to_string(),
        }),
    });

    let turn = &state.turns()[turn_index];
    assert_eq!(
        user_input_texts(turn),
        vec!["Initial prompt", "Steered follow-up"]
    );
    assert_eq!(
        narrative_texts(turn),
        vec![
            "user: Initial prompt",
            "assistant: Already rendered assistant output.",
            "user: Steered follow-up",
            "assistant: Assistant after steering.",
        ]
    );
}

#[test]
fn rejected_steering_fragment_can_be_removed_from_active_turn() {
    let mut state = ExecutionDetailState::default();
    let turn_index = state.begin_turn("Initial prompt".to_string());
    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "assistant_before".to_string(),
            phase: Some(ProtocolPhase::Commentary),
            text: "Already streamed output.".to_string(),
        }),
    });
    state.append_user_input_fragment(
        turn_index,
        execution_detail::UserInputFragment::text("Steered prompt"),
    );
    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_1".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "assistant_after".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "Later streamed output.".to_string(),
        }),
    });
    assert_eq!(
        narrative_texts(&state.turns()[turn_index]),
        vec![
            "user: Initial prompt",
            "assistant: Already streamed output.",
            "user: Steered prompt",
            "assistant: Later streamed output.",
        ]
    );

    let fragment_id = state.turns()[turn_index].user_input_fragments()[1].id;
    let affected =
        state.remove_user_input_fragments(&[(turn_index, fragment_id, "Steered prompt")]);

    assert_eq!(affected, vec![turn_index]);
    assert_eq!(
        user_input_texts(&state.turns()[turn_index]),
        vec!["Initial prompt"]
    );
    assert_eq!(
        narrative_texts(&state.turns()[turn_index]),
        vec![
            "user: Initial prompt",
            "assistant: Already streamed output.",
            "assistant: Later streamed output.",
        ]
    );
}

#[test]
fn rejected_steering_fragments_remove_by_identity_after_prior_removal() {
    let mut state = ExecutionDetailState::default();
    let turn_index = state.begin_turn("Initial prompt".to_string());
    state.append_user_input_fragment(
        turn_index,
        execution_detail::UserInputFragment::text("Repeat"),
    );
    state.append_user_input_fragment(
        turn_index,
        execution_detail::UserInputFragment::text("Repeat"),
    );
    let first_id = state.turns()[turn_index].user_input_fragments()[1].id;
    let second_id = state.turns()[turn_index].user_input_fragments()[2].id;

    state.remove_user_input_fragments(&[(turn_index, first_id, "Repeat")]);
    state.remove_user_input_fragments(&[(turn_index, second_id, "Repeat")]);

    assert_eq!(
        user_input_texts(&state.turns()[turn_index]),
        vec!["Initial prompt"]
    );
}

#[test]
fn execution_detail_ignores_events_from_non_active_turns() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Use an explorer subagent".to_string());

    assert_eq!(
        state.apply_stream_event(TurnStreamEvent::TurnStarted {
            thread_id: "parent_thread".to_string(),
            turn: TurnInfo {
                id: "parent_turn".to_string(),
                status: TurnStatus::InProgress,
                items: Vec::new(),
                error: None,
            },
        }),
        Some(0)
    );
    assert_eq!(
        state.apply_stream_event(TurnStreamEvent::ItemCompleted {
            thread_id: "child_thread".to_string(),
            turn_id: "child_turn".to_string(),
            item: ThreadItem::AgentMessage(AgentMessageItem {
                id: "child_msg".to_string(),
                phase: Some(ProtocolPhase::FinalAnswer),
                text: "Subagent handoff".to_string(),
            }),
        }),
        None
    );
    assert_eq!(
        state.apply_stream_event(TurnStreamEvent::TurnCompleted {
            thread_id: "child_thread".to_string(),
            turn: TurnInfo {
                id: "child_turn".to_string(),
                status: TurnStatus::Completed,
                items: Vec::new(),
                error: None,
            },
        }),
        None
    );

    let turn = &state.turns()[0];
    assert_eq!(state.last_turn_state(), LastTurnState::Working);
    assert_eq!(turn.status, TurnExecutionStatus::Running);
    assert_eq!(turn.thread_id.as_deref(), Some("parent_thread"));
    assert_eq!(turn.turn_id.as_deref(), Some("parent_turn"));
    assert!(turn.items.is_empty());

    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "parent_thread".to_string(),
        turn_id: "parent_turn".to_string(),
        item: ThreadItem::AgentMessage(AgentMessageItem {
            id: "parent_msg".to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
            text: "Parent final answer".to_string(),
        }),
    });
    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "parent_thread".to_string(),
        turn: TurnInfo {
            id: "parent_turn".to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    });

    let turn = &state.turns()[0];
    assert_eq!(state.last_turn_state(), LastTurnState::Ok);
    assert_eq!(turn.status, TurnExecutionStatus::Completed);
    assert_eq!(
        turn.terminal_assistant_item_id.as_deref(),
        Some("parent_msg")
    );
}

#[test]
fn execution_detail_loads_selected_thread_history() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.118.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "Explain the workspace",
            "source": "appServer",
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            },
            "turns": [{
                "id": "turn_1",
                "items": [
                    {
                        "id": "user_1",
                        "type": "userMessage",
                        "content": [{
                            "type": "text",
                            "text": "Explain the workspace"
                        }]
                    },
                    {
                        "id": "assistant_1",
                        "type": "agentMessage",
                        "phase": "final_answer",
                        "text": "The workspace contains Cargo.toml and a crates directory."
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let turn = &state.turns()[0];
    assert_eq!(turn.status, TurnExecutionStatus::Completed);
    assert_eq!(user_input_texts(turn), vec!["Explain the workspace"]);
    assert_eq!(turn.thread_id.as_deref(), Some("thread_1"));
    assert_eq!(turn.turn_id.as_deref(), Some("turn_1"));
    assert_eq!(
        turn.terminal_assistant_message()
            .map(|message| message.text.as_str()),
        Some("The workspace contains Cargo.toml and a crates directory.")
    );
    assert!(turn.awaiting_user_input);
}

#[test]
fn execution_detail_preserves_ordered_history_user_fragments() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "First fragment",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_1",
                "items": [
                    {
                        "id": "user_1",
                        "type": "userMessage",
                        "content": [
                            {
                                "type": "text",
                                "text": "First fragment"
                            },
                            {
                                "type": "text",
                                "text": "Second fragment"
                            }
                        ]
                    },
                    {
                        "id": "assistant_between",
                        "type": "agentMessage",
                        "phase": "commentary",
                        "text": "Assistant between user messages."
                    },
                    {
                        "id": "user_2",
                        "type": "userMessage",
                        "content": [{
                            "type": "text",
                            "text": "Second fragment"
                        }]
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    assert_eq!(
        user_input_texts(&state.turns()[0]),
        vec!["First fragment", "Second fragment", "Second fragment"]
    );
    assert_eq!(
        narrative_texts(&state.turns()[0]),
        vec![
            "user: First fragment",
            "user: Second fragment",
            "assistant: Assistant between user messages.",
            "user: Second fragment",
        ]
    );
}

#[test]
fn execution_detail_reconstructs_history_image_labels_as_markers() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "Look",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_1",
                "items": [{
                    "id": "user_1",
                    "type": "userMessage",
                    "content": [
                        {
                            "type": "text",
                            "text": "Look "
                        },
                        {
                            "type": "text",
                            "text": "Image A:"
                        },
                        {
                            "type": "localImage",
                            "path": "/tmp/beryl/a.png"
                        },
                        {
                            "type": "text",
                            "text": " then "
                        },
                        {
                            "type": "text",
                            "text": "[Image A]"
                        },
                        {
                            "type": "text",
                            "text": " now"
                        }
                    ]
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/a.png",
        TranscriptImageSourceResolution::available_asset("asset-a"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    assert_eq!(user_input_texts(turn), vec!["Look [A] then [A] now"]);
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].display_range(), 5..8);
    assert_eq!(markers[0].copy_text(), "[Image A]");
    assert_eq!(markers[0].source().asset_id(), Some("asset-a"));
    assert_eq!(
        markers[0].source().preview_state(),
        TranscriptImagePreviewState::Available
    );
    assert_eq!(markers[1].label(), "A");
    assert_eq!(markers[1].display_range(), 14..17);
    assert_eq!(markers[1].copy_text(), "[Image A]");
    assert_eq!(markers[1].source().asset_id(), Some("asset-a"));
    assert_ne!(markers[0].occurrence_id(), markers[1].occurrence_id());
    assert_eq!(
        turn.user_input_fragments()[0].backend_input(),
        &[
            beryl_backend::UserInput::text("Look "),
            beryl_backend::UserInput::text("Image A:"),
            beryl_backend::UserInput::local_image("/tmp/beryl/a.png"),
            beryl_backend::UserInput::text(" then "),
            beryl_backend::UserInput::text("[Image A]"),
            beryl_backend::UserInput::text(" now"),
        ]
    );
}

#[test]
fn execution_detail_consumes_merged_generated_image_label_suffix_before_image() {
    let prefix = "Straight-forward image paste: ";
    let response = response_with_user_content(json!([
        {
            "type": "text",
            "text": format!("{prefix}Image A:")
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/a.png"
        }
    ]));

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/a.png",
        TranscriptImageSourceResolution::available_asset("asset-a"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    assert_eq!(
        user_input_texts(turn),
        vec!["Straight-forward image paste: [A]"]
    );
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].display_range(), prefix.len()..prefix.len() + 3);
    assert_eq!(markers[0].copy_text(), "[Image A]");
    assert_eq!(markers[0].source().asset_id(), Some("asset-a"));
}

#[test]
fn execution_detail_consumes_delayed_generated_image_label_anchor_before_image() {
    let prefix = "Testing image paste: ";
    let suffix = "\nGoing to check how it looks in transcript before and after restart";
    let response = response_with_user_content(json!([
        {
            "type": "text",
            "text": format!("{prefix}Image B:{suffix}")
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/b.png"
        }
    ]));

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/b.png",
        TranscriptImageSourceResolution::available_asset("asset-b"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    let expected_text = format!("{prefix}[B]{suffix}");
    assert_eq!(user_input_texts(turn), vec![expected_text.as_str()]);
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].label(), "B");
    assert_eq!(markers[0].display_range(), prefix.len()..prefix.len() + 3);
    assert_eq!(markers[0].copy_text(), "[Image B]");
    assert_eq!(markers[0].source().asset_id(), Some("asset-b"));
}

#[test]
fn execution_detail_reconstructs_merged_generated_image_references_after_source() {
    let response = response_with_user_content(json!([
        {
            "type": "text",
            "text": "Image A:"
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/a.png"
        },
        {
            "type": "text",
            "text": " then [Image A] now"
        }
    ]));

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/a.png",
        TranscriptImageSourceResolution::available_asset("asset-a"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    assert_eq!(user_input_texts(turn), vec!["[A] then [A] now"]);
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].display_range(), 0..3);
    assert_eq!(markers[0].source().asset_id(), Some("asset-a"));
    assert_eq!(markers[1].label(), "A");
    assert_eq!(markers[1].display_range(), 9..12);
    assert_eq!(markers[1].source().asset_id(), Some("asset-a"));
}

#[test]
fn execution_detail_reconstructs_delayed_generated_references_after_anchor() {
    let response = response_with_user_content(json!([
        {
            "type": "text",
            "text": "Image B: then [Image B] now"
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/b.png"
        }
    ]));

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/b.png",
        TranscriptImageSourceResolution::available_asset("asset-b"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    assert_eq!(user_input_texts(turn), vec!["[B] then [B] now"]);
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].label(), "B");
    assert_eq!(markers[0].display_range(), 0..3);
    assert_eq!(markers[0].source().asset_id(), Some("asset-b"));
    assert_eq!(markers[1].label(), "B");
    assert_eq!(markers[1].display_range(), 9..12);
    assert_eq!(markers[1].source().asset_id(), Some("asset-b"));
}

#[test]
fn execution_detail_binds_multiple_delayed_anchors_to_images_in_order() {
    let response = response_with_user_content(json!([
        {
            "type": "text",
            "text": "First Image A: second Image B: done"
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/a.png"
        },
        {
            "type": "localImage",
            "path": "/tmp/beryl/b.png"
        }
    ]));

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/a.png",
        TranscriptImageSourceResolution::available_asset("asset-a"),
    );
    resolver.insert_local_path_resolution(
        "/tmp/beryl/b.png",
        TranscriptImageSourceResolution::available_asset("asset-b"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history_with_image_resolver(&response.thread, &resolver);

    let turn = &state.turns()[0];
    assert_eq!(user_input_texts(turn), vec!["First [A] second [B] done"]);
    let markers = turn.user_input_fragments()[0].image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].source().asset_id(), Some("asset-a"));
    assert_eq!(markers[1].label(), "B");
    assert_eq!(markers[1].source().asset_id(), Some("asset-b"));
}

#[test]
fn execution_detail_preserves_nonmatching_label_text_before_images() {
    let cases = [
        (
            json!([
                {
                    "type": "text",
                    "text": "Image A:"
                }
            ]),
            "Image A:",
            0,
        ),
        (
            json!([
                {
                    "type": "text",
                    "text": "Image a:"
                },
                {
                    "type": "localImage",
                    "path": "/tmp/beryl/a.png"
                }
            ]),
            "Image a:[A]",
            1,
        ),
        (
            json!([
                {
                    "type": "text",
                    "text": "[Image A]"
                },
                {
                    "type": "localImage",
                    "path": "/tmp/beryl/a.png"
                }
            ]),
            "[Image A][A]",
            1,
        ),
        (
            json!([
                {
                    "type": "localImage",
                    "path": "/tmp/beryl/a.png"
                },
                {
                    "type": "text",
                    "text": "Image A:"
                }
            ]),
            "[A]Image A:",
            1,
        ),
    ];

    for (content, expected_text, expected_marker_count) in cases {
        let response = response_with_user_content(content);
        let mut state = ExecutionDetailState::default();
        state.load_thread_history(&response.thread);

        let turn = &state.turns()[0];
        assert_eq!(user_input_texts(turn), vec![expected_text]);
        assert_eq!(
            turn.user_input_fragments()[0].image_markers().len(),
            expected_marker_count
        );
    }
}

#[test]
fn execution_detail_does_not_parse_arbitrary_history_marker_text_as_image() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "literal marker",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_1",
                "items": [{
                    "id": "user_1",
                    "type": "userMessage",
                    "content": [{
                        "type": "text",
                        "text": "literal [Image A] and [A]"
                    }]
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let fragment = &state.turns()[0].user_input_fragments()[0];
    assert_eq!(fragment.text, "literal [Image A] and [A]");
    assert!(fragment.image_markers().is_empty());
}

#[test]
fn prepended_history_page_uses_image_resolver_for_markers() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "latest",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_latest",
                "items": [{
                    "id": "user_latest",
                    "type": "userMessage",
                    "content": [{
                        "type": "text",
                        "text": "Latest"
                    }]
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();
    let older: TurnInfo = serde_json::from_value(json!({
        "id": "turn_older",
        "items": [{
            "id": "user_older",
            "type": "userMessage",
            "content": [
                {
                    "type": "text",
                    "text": "Older "
                },
                {
                    "type": "text",
                    "text": "Image B:"
                },
                {
                    "type": "localImage",
                    "path": "/tmp/beryl/b.png"
                }
            ]
        }],
        "status": "completed"
    }))
    .unwrap();

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/b.png",
        TranscriptImageSourceResolution::available_asset("asset-b"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let page =
        state.prepend_thread_history_page_with_image_resolver("thread_1", vec![older], &resolver);

    assert_eq!(page.added_count, 1);
    assert_eq!(page.turn_ids, vec!["turn_older".to_string()]);
    assert_eq!(user_input_texts(&state.turns()[0]), vec!["Older [B]"]);
    let marker = &state.turns()[0].user_input_fragments()[0].image_markers()[0];
    assert_eq!(marker.label(), "B");
    assert_eq!(marker.source().asset_id(), Some("asset-b"));
    assert_eq!(user_input_texts(&state.turns()[1]), vec!["Latest"]);
}

#[test]
fn restored_history_page_uses_image_resolver_for_markers() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "history",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_image",
                "items": [{
                    "id": "user_image",
                    "type": "userMessage",
                    "content": [
                        {
                            "type": "text",
                            "text": "Image C:"
                        },
                        {
                            "type": "localImage",
                            "path": "/tmp/beryl/c.png"
                        }
                    ]
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();
    let restored_turn: TurnInfo = serde_json::from_value(json!({
        "id": "turn_image",
        "items": [{
            "id": "user_image",
            "type": "userMessage",
            "content": [
                {
                    "type": "text",
                    "text": "Image C:"
                },
                {
                    "type": "localImage",
                    "path": "/tmp/beryl/c.png"
                }
            ]
        }],
        "status": "completed"
    }))
    .unwrap();

    let mut resolver = TranscriptImagePathResolver::default();
    resolver.insert_local_path_resolution(
        "/tmp/beryl/c.png",
        TranscriptImageSourceResolution::available_asset("asset-c"),
    );
    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);
    let replacements = state.release_history_range(0..1);
    assert_eq!(replacements.len(), 1);
    assert!(state.turns()[0].is_released_history_placeholder());

    let replacements = state.restore_history_page_with_image_resolver(
        "thread_1",
        0,
        &["turn_image".to_string()],
        vec![restored_turn],
        &resolver,
    );

    assert_eq!(replacements.len(), 1);
    assert_eq!(user_input_texts(&state.turns()[0]), vec!["[C]"]);
    let marker = &state.turns()[0].user_input_fragments()[0].image_markers()[0];
    assert_eq!(marker.label(), "C");
    assert_eq!(marker.source().asset_id(), Some("asset-c"));
}

#[test]
fn execution_detail_reset_clears_loaded_history() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("List files".to_string());
    state.reset();

    assert!(state.turns().is_empty());
    assert_eq!(state.last_turn_state(), LastTurnState::Unknown);
}

#[test]
fn execution_detail_preserves_live_turn_records() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Inspect the workspace".to_string());

    let turn = &state.turns()[0];
    assert_eq!(user_input_texts(turn), vec!["Inspect the workspace"]);
    assert_eq!(turn.status, TurnExecutionStatus::Starting);
    assert_eq!(turn.terminal_assistant_item_id, None);

    state.finish_turn_failure("backend unavailable");

    let turn = &state.turns()[0];
    assert_eq!(turn.status, TurnExecutionStatus::Failed);
    assert_eq!(turn.error_message.as_deref(), Some("backend unavailable"));
}

#[test]
fn execution_detail_projects_last_turn_state() {
    let mut state = ExecutionDetailState::default();
    assert_eq!(state.last_turn_state(), LastTurnState::Unknown);
    assert_eq!(state.last_turn_state().label(), "Unknown");

    state.begin_turn("Inspect the workspace".to_string());
    assert_eq!(state.last_turn_state(), LastTurnState::Working);
    assert_eq!(state.last_turn_state().label(), "working");

    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });
    state.apply_stream_event(TurnStreamEvent::TurnCompleted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_1".to_string(),
            status: TurnStatus::Completed,
            items: Vec::new(),
            error: None,
        },
    });
    assert_eq!(state.last_turn_state(), LastTurnState::Ok);
    assert_eq!(state.last_turn_state().label(), "ok");

    state.begin_turn("Run a broken command".to_string());
    state.finish_turn_failure("backend unavailable");
    assert_eq!(state.last_turn_state(), LastTurnState::Error);
    assert_eq!(state.last_turn_state().label(), "error");
}

#[test]
fn context_compaction_history_turn_is_preserved_in_loaded_history() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.118.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "compaction",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [{
                "id": "turn_compact",
                "items": [{
                    "id": "item_compact",
                    "type": "contextCompaction"
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let turn = &state.turns()[0];
    assert_eq!(state.turns().len(), 1);
    assert!(turn.user_input_fragments().is_empty());
    assert_eq!(turn.terminal_assistant_item_id, None);
    let [ExecutionItem::Generic(item)] = turn.items.as_slice() else {
        panic!("expected context compaction history turn to keep one generic item");
    };
    assert_eq!(item.item_type, "contextCompaction");
    assert!(item.complete);
}

#[test]
fn image_generation_history_turn_is_preserved_as_generated_image_item() {
    let large_result = "A".repeat(300 * 1024);
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "image",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [{
                "id": "turn_image",
                "items": [{
                    "id": "image_generation_1",
                    "type": "imageGeneration",
                    "status": "generating",
                    "revisedPrompt": "A small glass cat",
                    "result": large_result,
                    "savedPath": "C:/work/beryl/cat.png"
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let [ExecutionItem::GeneratedImage(item)] = state.turns()[0].items.as_slice() else {
        panic!("expected imageGeneration history item to stay typed");
    };
    assert_eq!(item.id, "image_generation_1");
    assert_eq!(item.status.as_deref(), Some("generating"));
    assert_eq!(item.revised_prompt.as_deref(), Some("A small glass cat"));
    assert!(item.result.is_none());
    assert_eq!(item.saved_path.as_deref(), Some("C:/work/beryl/cat.png"));
    assert!(item.complete);
}

#[test]
fn image_generation_history_without_saved_path_keeps_only_bounded_inline_result() {
    let large_result = "A".repeat(300 * 1024);
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "image",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [{
                "id": "turn_image",
                "items": [
                    {
                        "id": "small_inline",
                        "type": "imageGeneration",
                        "status": "completed",
                        "revisedPrompt": "Tiny inline image",
                        "result": "iVBORw0KGgo="
                    },
                    {
                        "id": "large_inline",
                        "type": "imageGeneration",
                        "status": "completed",
                        "revisedPrompt": "Huge inline image",
                        "result": large_result
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let [
        ExecutionItem::GeneratedImage(small),
        ExecutionItem::GeneratedImage(large),
    ] = state.turns()[0].items.as_slice()
    else {
        panic!("expected generated image history items to stay typed");
    };
    assert_eq!(
        small.result.as_ref().map(|result| result.as_str()),
        Some("iVBORw0KGgo=")
    );
    assert!(small.saved_path.is_none());
    assert!(large.result.is_none());
    assert!(large.saved_path.is_none());
}

#[test]
fn live_image_generation_without_saved_path_keeps_only_bounded_inline_result() {
    let mut state = ExecutionDetailState::default();
    state.begin_turn("Generate an image".to_string());
    state.apply_stream_event(TurnStreamEvent::TurnStarted {
        thread_id: "thread_1".to_string(),
        turn: TurnInfo {
            id: "turn_image".to_string(),
            status: TurnStatus::InProgress,
            items: Vec::new(),
            error: None,
        },
    });

    state.apply_stream_event(TurnStreamEvent::ItemCompleted {
        thread_id: "thread_1".to_string(),
        turn_id: "turn_image".to_string(),
        item: ThreadItem::ImageGeneration(ImageGenerationItem {
            id: "large_inline".to_string(),
            status: Some("completed".to_string()),
            revised_prompt: Some("Huge inline image".to_string()),
            result: Some("A".repeat(MAX_INLINE_GENERATED_IMAGE_RESULT_BYTES + 1)),
            saved_path: None,
        }),
    });

    let [ExecutionItem::GeneratedImage(image)] = state.turns()[0].items.as_slice() else {
        panic!("expected generated image item");
    };
    assert_eq!(image.id, "large_inline");
    assert!(image.result.is_none());
    assert!(image.saved_path.is_none());
}

#[test]
fn history_reload_preserves_generated_image_and_markdown_image_reference() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "image reference",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [{
                "id": "turn_media",
                "items": [
                    {
                        "id": "assistant_markdown_image",
                        "type": "agentMessage",
                        "phase": "final_answer",
                        "text": "Before ![cat](images/cat.png) after"
                    },
                    {
                        "id": "generated_cat",
                        "type": "imageGeneration",
                        "status": "completed",
                        "revisedPrompt": "A striped glass cat",
                        "result": "iVBORw0KGgo=",
                        "savedPath": "C:/work/beryl/generated-cat.png"
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let mut state = ExecutionDetailState::default();
    state.load_thread_history(&response.thread);

    let [
        ExecutionItem::AgentMessage(message),
        ExecutionItem::GeneratedImage(image),
    ] = state.turns()[0].items.as_slice()
    else {
        panic!("expected Markdown message and generated image to stay in narrative order");
    };
    assert_eq!(message.text, "Before ![cat](images/cat.png) after");
    assert_eq!(image.id, "generated_cat");
    assert_eq!(image.revised_prompt.as_deref(), Some("A striped glass cat"));
    assert_eq!(
        image.saved_path.as_deref(),
        Some("C:/work/beryl/generated-cat.png")
    );
}

fn user_input_texts(turn: &TurnExecutionRecord) -> Vec<&str> {
    turn.user_input_fragments()
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}

fn response_with_user_content(content: serde_json::Value) -> ThreadSessionResponse {
    serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_1",
            "modelProvider": "openai",
            "preview": "image",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_1",
                "items": [{
                    "id": "user_1",
                    "type": "userMessage",
                    "content": content
                }],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap()
}

fn narrative_texts(turn: &TurnExecutionRecord) -> Vec<String> {
    turn.narrative_entries()
        .iter()
        .filter_map(|entry| match entry {
            TurnNarrativeEntry::UserInput { fragment_id } => turn
                .user_input_fragment_by_id(*fragment_id)
                .map(|(_, fragment)| format!("user: {}", fragment.text)),
            TurnNarrativeEntry::Item { item_id } => {
                turn.item_by_id(item_id).and_then(|item| match item {
                    ExecutionItem::AgentMessage(message) => {
                        Some(format!("assistant: {}", message.text))
                    }
                    _ => None,
                })
            }
        })
        .collect()
}
