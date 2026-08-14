use std::path::PathBuf;

use beryl_backend::{
    AccountRateLimitsResponse, DynamicToolCallOutputContentItem, DynamicToolCallResponse,
    JsonRpcError, NonSteerableTurnKind, ProtocolPhase, SortDirection, ThreadForkResponse,
    ThreadItem, ThreadListResponse, ThreadReadOptions, ThreadReadResponse, ThreadResumeOptions,
    ThreadRollbackResponse, ThreadSessionResponse, ThreadStatus, ThreadTurnsListOptions,
    ThreadTurnsListResponse, ThreadUnsubscribeResponse, ThreadUnsubscribeStatus, ToolActivityEvent,
    ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource, TurnStartOptions,
    TurnStartResponse, TurnStatus, TurnSteerResponse, TurnStreamEvent, UserInput,
    active_turn_not_steerable_error, parse_approval_request, parse_dynamic_tool_call_request,
    parse_turn_stream_event,
};
use serde_json::{Value, json};

fn parse_tool_activity(method: &str, item: Value) -> ToolActivityEvent {
    parse_turn_stream_event(
        method,
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "item": item
        })),
    )
    .unwrap()
    .unwrap()
    .tool_activity()
    .expect("expected normalized tool activity")
}

fn parse_item_activity(method: &str, item: Value) -> ToolActivityEvent {
    parse_turn_stream_event(
        method,
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "item": item
        })),
    )
    .unwrap()
    .unwrap()
    .activity()
    .expect("expected normalized activity")
}

#[test]
fn thread_session_response_deserializes_runtime_status() {
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
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_123");
    assert_eq!(response.metadata().model.as_deref(), Some("gpt-5.4"));
    assert_eq!(
        response.metadata().model_provider.as_deref(),
        Some("openai")
    );
    assert_eq!(response.metadata().reasoning_effort, None);
    assert!(response.thread.status.waiting_on_user_input());
}

#[test]
fn thread_session_response_preserves_reasoning_effort_metadata() {
    let response: ThreadSessionResponse = serde_json::from_value(json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "C:/work/beryl",
        "model": "gpt-5.4",
        "modelProvider": "openai",
        "reasoningEffort": "high",
        "sandbox": {
            "mode": "danger-full-access",
            "networkAccess": true
        },
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let metadata = response.metadata();
    assert_eq!(metadata.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(metadata.model_provider.as_deref(), Some("openai"));
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("high"));
}

#[test]
fn thread_session_response_preserves_agent_nickname_metadata() {
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
            "agentNickname": "Hooke",
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": "subAgent",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_child");
    assert_eq!(
        response.thread.summary().agent_nickname.as_deref(),
        Some("Hooke")
    );
}

#[test]
fn thread_session_response_preserves_fork_parent_metadata() {
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
            "forkedFromId": "thread_parent",
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Forked work",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_child");
    assert_eq!(
        response.thread.summary().forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn thread_session_response_derives_agent_nickname_from_subagent_source_metadata() {
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
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": {
                "subAgent": {
                    "thread_spawn": {
                        "agent_nickname": "Gauss",
                        "agent_role": "explorer",
                        "depth": 1,
                        "parent_thread_id": "thread_parent"
                    }
                }
            },
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_child");
    assert_eq!(
        response.thread.summary().agent_nickname.as_deref(),
        Some("Gauss")
    );
}

#[test]
fn thread_list_response_preserves_agent_nickname_metadata() {
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [{
            "agentNickname": "Hooke",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "updatedAt": 2
        }],
        "nextCursor": null
    }))
    .unwrap();

    assert_eq!(response.data[0].id, "thread_child");
    assert_eq!(response.data[0].agent_nickname.as_deref(), Some("Hooke"));
}

#[test]
fn thread_list_response_derives_agent_nickname_from_subagent_source_metadata() {
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [{
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": {
                "subAgent": {
                    "thread_spawn": {
                        "agent_nickname": "Gauss",
                        "agent_role": "explorer",
                        "depth": 1,
                        "parent_thread_id": "thread_parent"
                    }
                }
            },
            "updatedAt": 2
        }],
        "nextCursor": null
    }))
    .unwrap();

    assert_eq!(response.data[0].id, "thread_child");
    assert_eq!(response.data[0].agent_nickname.as_deref(), Some("Gauss"));
}

#[test]
fn thread_list_response_preserves_fork_parent_metadata() {
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [
            {
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "id": "thread_parent",
                "modelProvider": "openai",
                "preview": "Parent work",
                "updatedAt": 2
            },
            {
                "createdAt": 3,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "forkedFromId": "thread_parent",
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Child work",
                "updatedAt": 4
            }
        ],
        "nextCursor": null
    }))
    .unwrap();

    assert_eq!(response.data[0].forked_from_id, None);
    assert_eq!(response.data[1].id, "thread_child");
    assert_eq!(
        response.data[1].forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn thread_list_response_treats_malformed_fork_parent_metadata_as_absent() {
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [
            {
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "forkedFromId": {"id": "thread_parent"},
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Child work",
                "updatedAt": 2
            },
            {
                "createdAt": 3,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "forkedFromId": 42,
                "id": "thread_other_child",
                "modelProvider": "openai",
                "preview": "Other child work",
                "updatedAt": 4
            }
        ],
        "nextCursor": null
    }))
    .unwrap();

    assert_eq!(response.data[0].forked_from_id, None);
    assert_eq!(response.data[1].forked_from_id, None);
}

#[test]
fn thread_read_response_can_supply_fork_parent_when_list_row_is_null() {
    let list_response: ThreadListResponse = serde_json::from_value(json!({
        "data": [
            {
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "forkedFromId": null,
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Child work",
                "updatedAt": 2
            }
        ]
    }))
    .unwrap();
    let read_response: ThreadReadResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "forkedFromId": "thread_parent",
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Child work",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(list_response.data[0].forked_from_id, None);
    assert_eq!(
        read_response
            .read_metadata()
            .thread
            .forked_from_id
            .as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn thread_read_response_preserves_agent_nickname_metadata() {
    let response: ThreadReadResponse = serde_json::from_value(json!({
        "thread": {
            "agentNickname": "Hooke",
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": "subAgent",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_child");
    assert_eq!(
        response.thread.summary().agent_nickname.as_deref(),
        Some("Hooke")
    );
}

#[test]
fn thread_read_response_preserves_fork_parent_metadata() {
    let response: ThreadReadResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "forkedFromId": "thread_parent",
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Forked work",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(
        response.thread.summary().forked_from_id.as_deref(),
        Some("thread_parent")
    );
    assert_eq!(
        response.read_metadata().thread.forked_from_id.as_deref(),
        Some("thread_parent")
    );
}

#[test]
fn thread_read_response_derives_agent_nickname_from_subagent_source_metadata() {
    let response: ThreadReadResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": {
                "subAgent": {
                    "thread_spawn": {
                        "agent_nickname": "Gauss",
                        "agent_role": "explorer",
                        "depth": 1,
                        "parent_thread_id": "thread_parent"
                    }
                }
            },
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_child");
    assert_eq!(
        response.thread.summary().agent_nickname.as_deref(),
        Some("Gauss")
    );
}

#[test]
fn thread_read_response_preserves_runtime_metadata_when_exposed() {
    let response: ThreadReadResponse = serde_json::from_value(json!({
        "model": "gpt-5.5",
        "modelProvider": "openai",
        "reasoningEffort": "xhigh",
        "thread": {
            "agentNickname": "Curie",
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Subagent work",
            "source": "subAgent",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let metadata = response.metadata();
    assert_eq!(metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(metadata.model_provider.as_deref(), Some("openai"));
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("xhigh"));

    let read_metadata = response.read_metadata();
    assert_eq!(read_metadata.thread.id, "thread_child");
    assert_eq!(
        read_metadata.thread.agent_nickname.as_deref(),
        Some("Curie")
    );
    assert_eq!(
        read_metadata.session_metadata.reasoning_effort.as_deref(),
        Some("xhigh")
    );
}

#[test]
fn thread_branch_responses_preserve_fork_parent_metadata() {
    let fork_response: ThreadForkResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "forkedFromId": "thread_parent",
            "id": "thread_child",
            "modelProvider": "openai",
            "preview": "Forked work",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();
    let rollback_response: ThreadRollbackResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.128.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "forkedFromId": "thread_source",
            "id": "thread_rollback",
            "modelProvider": "openai",
            "preview": "Rollback work",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(
        fork_response.thread.summary().forked_from_id.as_deref(),
        Some("thread_parent")
    );
    assert_eq!(
        rollback_response.thread.summary().forked_from_id.as_deref(),
        Some("thread_source")
    );
}

#[test]
fn token_usage_notification_parses_from_turn_stream() {
    let event = parse_turn_stream_event(
        "thread/tokenUsage/updated",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "tokenUsage": {
                "last": {
                    "cachedInputTokens": 5,
                    "inputTokens": 250,
                    "outputTokens": 10,
                    "reasoningOutputTokens": 2,
                    "totalTokens": 262
                },
                "total": {
                    "cachedInputTokens": 50,
                    "inputTokens": 900,
                    "outputTokens": 70,
                    "reasoningOutputTokens": 30,
                    "totalTokens": 1000
                },
                "modelContextWindow": 1000
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::TokenUsageUpdated {
        thread_id,
        turn_id,
        token_usage,
    } = event
    else {
        panic!("expected token usage update");
    };

    assert_eq!(thread_id, "thread_123");
    assert_eq!(turn_id, "turn_123");
    assert_eq!(token_usage.last.input_tokens, 250);
    assert_eq!(token_usage.total.input_tokens, 900);
    assert_eq!(token_usage.model_context_window, Some(1000));
}

#[test]
fn account_rate_limits_notification_parses_from_turn_stream() {
    let event = parse_turn_stream_event(
        "account/rateLimits/updated",
        Some(json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 15,
                    "windowDurationMins": 1440,
                    "resetsAt": 1_770_000_000
                },
                "secondary": {
                    "usedPercent": 55,
                    "windowDurationMins": 10080
                }
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::AccountRateLimitsUpdated { rate_limits } = event else {
        panic!("expected account rate limits update");
    };

    let daily = rate_limits.primary.unwrap();
    assert_eq!(daily.used_percent, 15);
    assert_eq!(daily.window_duration_mins, Some(1440));
    assert_eq!(daily.resets_at, Some(1_770_000_000));

    let weekly = rate_limits.secondary.unwrap();
    assert_eq!(weekly.used_percent, 55);
    assert_eq!(weekly.window_duration_mins, Some(10080));
    assert_eq!(weekly.resets_at, None);
}

#[test]
fn account_rate_limits_read_response_deserializes_multi_bucket_view() {
    let response: AccountRateLimitsResponse = serde_json::from_value(json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 55,
                "windowDurationMins": 10080
            }
        },
        "rateLimitsByLimitId": {
            "codex": {
                "limitId": "codex",
                "limitName": "Codex",
                "primary": {
                    "usedPercent": 15,
                    "windowDurationMins": 1440
                },
                "secondary": {
                    "usedPercent": 55,
                    "windowDurationMins": 10080
                }
            }
        }
    }))
    .unwrap();

    assert_eq!(
        response.rate_limits.primary.unwrap().window_duration_mins,
        Some(10080)
    );
    let mut by_limit_id = response.rate_limits_by_limit_id.unwrap();
    let codex = by_limit_id.remove("codex").unwrap();
    assert_eq!(codex.limit_id.as_deref(), Some("codex"));
    assert_eq!(codex.limit_name.as_deref(), Some("Codex"));
    assert_eq!(codex.primary.unwrap().used_percent, 15);
    assert_eq!(codex.secondary.unwrap().used_percent, 55);
}

#[test]
fn thread_name_notification_parses_from_turn_stream() {
    let event = parse_turn_stream_event(
        "thread/name/updated",
        Some(json!({
            "threadId": "thread_123",
            "threadName": "Backend-generated title"
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadNameUpdated {
        thread_id,
        thread_name,
    } = event
    else {
        panic!("expected thread name update");
    };

    assert_eq!(thread_id, "thread_123");
    assert_eq!(thread_name.as_deref(), Some("Backend-generated title"));
}

#[test]
fn thread_name_notification_preserves_cleared_name() {
    let event = parse_turn_stream_event(
        "thread/name/updated",
        Some(json!({
            "threadId": "thread_123",
            "threadName": null
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadNameUpdated {
        thread_id,
        thread_name,
    } = event
    else {
        panic!("expected thread name update");
    };

    assert_eq!(thread_id, "thread_123");
    assert_eq!(thread_name, None);
}

#[test]
fn thread_closed_notification_parses_from_turn_stream() {
    let event = parse_turn_stream_event(
        "thread/closed",
        Some(json!({
            "threadId": "thread_maintenance"
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadClosed { thread_id } = event else {
        panic!("expected thread closed");
    };

    assert_eq!(thread_id, "thread_maintenance");
}

#[test]
fn thread_started_notification_preserves_agent_nickname_metadata() {
    let event = parse_turn_stream_event(
        "thread/started",
        Some(json!({
            "thread": {
                "agentNickname": "Hooke",
                "agentRole": "explorer",
                "cliVersion": "0.125.0",
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Subagent work",
                "source": "subAgent",
                "status": {"type": "active", "activeFlags": []},
                "turns": [],
                "updatedAt": 2
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadStarted { thread } = event else {
        panic!("expected thread started");
    };

    assert_eq!(thread.id, "thread_child");
    assert_eq!(thread.agent_nickname.as_deref(), Some("Hooke"));
}

#[test]
fn thread_started_notification_preserves_fork_parent_metadata() {
    let event = parse_turn_stream_event(
        "thread/started",
        Some(json!({
            "thread": {
                "cliVersion": "0.128.0",
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "forkedFromId": "thread_parent",
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Forked work",
                "source": "appServer",
                "status": {"type": "active", "activeFlags": []},
                "turns": [],
                "updatedAt": 2
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadStarted { thread } = event else {
        panic!("expected thread started");
    };

    assert_eq!(thread.id, "thread_child");
    assert_eq!(thread.forked_from_id.as_deref(), Some("thread_parent"));
}

#[test]
fn thread_started_notification_derives_agent_nickname_from_subagent_source_metadata() {
    let event = parse_turn_stream_event(
        "thread/started",
        Some(json!({
            "thread": {
                "agentRole": "explorer",
                "cliVersion": "0.125.0",
                "createdAt": 1,
                "cwd": "C:/work/beryl",
                "ephemeral": false,
                "id": "thread_child",
                "modelProvider": "openai",
                "preview": "Subagent work",
                "source": {
                    "subAgent": {
                        "thread_spawn": {
                            "agent_nickname": "Gauss",
                            "agent_role": "explorer",
                            "depth": 1,
                            "parent_thread_id": "thread_parent"
                        }
                    }
                },
                "status": {"type": "active", "activeFlags": []},
                "turns": [],
                "updatedAt": 2
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ThreadStarted { thread } = event else {
        panic!("expected thread started");
    };

    assert_eq!(thread.id, "thread_child");
    assert_eq!(thread.agent_nickname.as_deref(), Some("Gauss"));
}

#[test]
fn collab_spawn_end_event_derives_agent_label_update() {
    let event = parse_turn_stream_event(
        "codex/event/collab_agent_spawn_end",
        Some(json!({
            "id": "turn_parent",
            "conversationId": "thread_parent",
            "msg": {
                "type": "collab_agent_spawn_end",
                "call_id": "call_1",
                "sender_thread_id": "thread_parent",
                "new_thread_id": "thread_child",
                "new_agent_nickname": "Gauss",
                "new_agent_role": "explorer",
                "prompt": "Inspect activity labels",
                "status": "running"
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::AgentLabelUpdated { thread_id, label } = event else {
        panic!("expected agent label update");
    };

    assert_eq!(thread_id, "thread_child");
    assert_eq!(label, "Gauss");
}

#[test]
fn tool_activity_normalizes_operational_item_started_notifications() {
    struct ExpectedActivity<'a> {
        item: Value,
        item_id: &'a str,
        item_type: &'a str,
        source: ToolActivitySource,
        raw_tool_name: Option<&'a str>,
        raw_tool_server: Option<&'a str>,
        raw_tool_namespace: Option<&'a str>,
        raw_resource_uri: Option<&'a str>,
        raw_command: Option<&'a str>,
        command_exec_process_id: Option<&'a str>,
        raw_item_status: Option<&'a str>,
        receiver_thread_ids: Vec<&'a str>,
    }

    let cases = vec![
        ExpectedActivity {
            item: json!({
                "id": "cmd_1",
                "type": "commandExecution",
                "command": "rg activity",
                "cwd": "C:/work/beryl",
                "processId": "proc_123",
                "status": "inProgress"
            }),
            item_id: "cmd_1",
            item_type: "commandExecution",
            source: ToolActivitySource::CommandExecution,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: Some("rg activity"),
            command_exec_process_id: Some("proc_123"),
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "patch_1",
                "type": "fileChange",
                "changes": [],
                "status": "inProgress"
            }),
            item_id: "patch_1",
            item_type: "fileChange",
            source: ToolActivitySource::FileChange,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "mcp_1",
                "type": "mcpToolCall",
                "arguments": {"path": "src/lib.rs"},
                "server": "filesystem",
                "tool": "read_file",
                "mcpAppResourceUri": "file:///src/lib.rs",
                "status": "inProgress"
            }),
            item_id: "mcp_1",
            item_type: "mcpToolCall",
            source: ToolActivitySource::McpToolCall,
            raw_tool_name: Some("read_file"),
            raw_tool_server: Some("filesystem"),
            raw_tool_namespace: None,
            raw_resource_uri: Some("file:///src/lib.rs"),
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "dyn_1",
                "type": "dynamicToolCall",
                "arguments": {"q": "activity"},
                "namespace": "web",
                "tool": "search_query",
                "status": "inProgress"
            }),
            item_id: "dyn_1",
            item_type: "dynamicToolCall",
            source: ToolActivitySource::DynamicToolCall,
            raw_tool_name: Some("search_query"),
            raw_tool_server: None,
            raw_tool_namespace: Some("web"),
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "agent_1",
                "type": "collabAgentToolCall",
                "agentsStates": {},
                "receiverThreadIds": ["thread_child"],
                "senderThreadId": "thread_123",
                "status": "inProgress",
                "tool": "spawnAgent"
            }),
            item_id: "agent_1",
            item_type: "collabAgentToolCall",
            source: ToolActivitySource::CollabAgentToolCall,
            raw_tool_name: Some("spawnAgent"),
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: vec!["thread_child"],
        },
        ExpectedActivity {
            item: json!({
                "id": "web_1",
                "type": "webSearch",
                "query": "Codex App Server"
            }),
            item_id: "web_1",
            item_type: "webSearch",
            source: ToolActivitySource::WebSearch,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: None,
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "image_view_1",
                "type": "imageView",
                "path": "C:/work/beryl/screenshot.png"
            }),
            item_id: "image_view_1",
            item_type: "imageView",
            source: ToolActivitySource::ImageView,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: None,
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "image_generation_1",
                "type": "imageGeneration",
                "result": "",
                "status": "inProgress"
            }),
            item_id: "image_generation_1",
            item_type: "imageGeneration",
            source: ToolActivitySource::ImageGeneration,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: Some("inProgress"),
            receiver_thread_ids: Vec::new(),
        },
        ExpectedActivity {
            item: json!({
                "id": "compact_1",
                "type": "contextCompaction"
            }),
            item_id: "compact_1",
            item_type: "contextCompaction",
            source: ToolActivitySource::ContextCompaction,
            raw_tool_name: None,
            raw_tool_server: None,
            raw_tool_namespace: None,
            raw_resource_uri: None,
            raw_command: None,
            command_exec_process_id: None,
            raw_item_status: None,
            receiver_thread_ids: Vec::new(),
        },
    ];

    for expected in cases {
        let activity = parse_tool_activity("item/started", expected.item);
        assert_eq!(activity.thread_id, "thread_123");
        assert_eq!(activity.turn_id, "turn_123");
        assert_eq!(activity.item_id, expected.item_id);
        assert_eq!(activity.item_type, expected.item_type);
        assert_eq!(activity.source, expected.source);
        assert_eq!(activity.lifecycle, ToolActivityLifecycle::Started);
        assert_eq!(activity.raw_tool_name.as_deref(), expected.raw_tool_name);
        assert_eq!(
            activity.raw_tool_server.as_deref(),
            expected.raw_tool_server
        );
        assert_eq!(
            activity.raw_tool_namespace.as_deref(),
            expected.raw_tool_namespace
        );
        assert_eq!(
            activity.raw_resource_uri.as_deref(),
            expected.raw_resource_uri
        );
        assert_eq!(activity.raw_command.as_deref(), expected.raw_command);
        assert_eq!(
            activity.command_exec_process_id.as_deref(),
            expected.command_exec_process_id
        );
        assert_eq!(
            activity.raw_item_status.as_deref(),
            expected.raw_item_status
        );
        assert_eq!(activity.receiver_thread_ids, expected.receiver_thread_ids);
        assert!(activity.collab_agent_spawn_metadata.is_none());
        assert!(activity.agent_label_updates.is_empty());
    }
}

#[test]
fn file_change_tool_activity_counts_unique_files_and_diff_lines() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "patch_1",
            "type": "fileChange",
            "changes": [
                {
                    "path": "src/lib.rs",
                    "diff": "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n context\n-old\n+new\n+\n",
                    "kind": {"type": "update"}
                },
                {
                    "path": "src/lib.rs",
                    "diff": "@@ -8,2 +9,2 @@\n-duplicate\n+duplicate\n",
                    "kind": {"type": "update"}
                },
                {
                    "path": "src/main.rs",
                    "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n-deleted\n+added\n++literal\n--literal\n",
                    "kind": {"type": "add"}
                }
            ],
            "status": "inProgress"
        }),
    );

    assert_eq!(
        activity.file_change_summary,
        Some(ToolActivityFileChangeSummary {
            file_count: 2,
            additions: 5,
            deletions: 4,
            single_file_path: None,
        })
    );
    assert_eq!(activity.source, ToolActivitySource::FileChange);
    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Started);
    assert_eq!(activity.raw_item_status.as_deref(), Some("inProgress"));
}

#[test]
fn file_change_tool_activity_preserves_single_unique_file_path() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "patch_1",
            "type": "fileChange",
            "changes": [
                {
                    "path": "src/lib.rs",
                    "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+\n",
                    "kind": {"type": "update"}
                },
                {
                    "path": "src/lib.rs",
                    "diff": "@@ -8,2 +9,2 @@\n-duplicate\n+duplicate\n",
                    "kind": {"type": "update"}
                }
            ],
            "status": "inProgress"
        }),
    );

    assert_eq!(
        activity.file_change_summary,
        Some(ToolActivityFileChangeSummary {
            file_count: 1,
            additions: 3,
            deletions: 2,
            single_file_path: Some(PathBuf::from("src/lib.rs")),
        })
    );
}

#[test]
fn file_change_tool_activity_reports_zero_summary_for_empty_changes() {
    let activity = parse_tool_activity(
        "item/completed",
        json!({
            "id": "patch_1",
            "type": "fileChange",
            "changes": [],
            "status": "completed"
        }),
    );

    assert_eq!(
        activity.file_change_summary,
        Some(ToolActivityFileChangeSummary {
            file_count: 0,
            additions: 0,
            deletions: 0,
            single_file_path: None,
        })
    );
    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Completed);
    assert_eq!(activity.raw_item_status.as_deref(), Some("completed"));
}

#[test]
fn collab_agent_tool_activity_does_not_invent_labels_from_status_states() {
    let activity = parse_tool_activity(
        "item/completed",
        json!({
            "id": "agent_1",
            "type": "collabAgentToolCall",
            "agentsStates": {
                "thread_child": {"status": "running", "message": null},
                "thread_other": {"status": "completed", "message": "done"}
            },
            "receiverThreadIds": ["thread_child", "thread_other"],
            "senderThreadId": "thread_parent",
            "status": "completed",
            "tool": "wait"
        }),
    );

    assert!(activity.agent_label_updates.is_empty());
    assert!(activity.collab_agent_spawn_metadata.is_none());
    assert_eq!(
        activity.receiver_thread_ids,
        vec!["thread_child".to_string(), "thread_other".to_string()]
    );
}

#[test]
fn collab_agent_tool_activity_preserves_spawn_model_metadata() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "agent_1",
            "type": "collabAgentToolCall",
            "agentsStates": {},
            "model": "gpt-5.5",
            "reasoningEffort": "xhigh",
            "receiverThreadIds": ["thread_child"],
            "senderThreadId": "thread_parent",
            "status": "inProgress",
            "tool": "spawnAgent"
        }),
    );

    assert_eq!(activity.source, ToolActivitySource::CollabAgentToolCall);
    let metadata = activity
        .collab_agent_spawn_metadata
        .as_ref()
        .expect("expected collab spawn metadata");
    assert_eq!(metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("xhigh"));
    assert_eq!(
        activity.receiver_thread_ids,
        vec!["thread_child".to_string()]
    );
}

#[test]
fn collab_agent_tool_activity_trims_and_drops_empty_spawn_metadata() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "agent_1",
            "type": "collabAgentToolCall",
            "agentsStates": {},
            "model": "  gpt-5.5  ",
            "reasoningEffort": "   ",
            "receiverThreadIds": ["thread_child"],
            "senderThreadId": "thread_parent",
            "status": "inProgress",
            "tool": "spawnAgent"
        }),
    );

    let metadata = activity
        .collab_agent_spawn_metadata
        .as_ref()
        .expect("expected collab spawn metadata");
    assert_eq!(metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(metadata.reasoning_effort.as_deref(), None);
}

#[test]
fn tool_activity_ignores_spawn_model_metadata_on_non_collab_items() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "dyn_1",
            "type": "dynamicToolCall",
            "arguments": {},
            "model": "gpt-5.5",
            "namespace": "web",
            "reasoningEffort": "xhigh",
            "status": "inProgress",
            "tool": "search_query"
        }),
    );

    assert_eq!(activity.source, ToolActivitySource::DynamicToolCall);
    assert!(activity.collab_agent_spawn_metadata.is_none());
}

#[test]
fn collab_agent_tool_activity_extracts_legacy_agent_label_updates() {
    let activity = parse_tool_activity(
        "item/started",
        json!({
            "id": "agent_1",
            "type": "collabAgentToolCall",
            "agentsStates": {
                "thread_child": {"agentNickname": "Hooke"},
                "agent_state_2": {"threadId": "thread_other", "nickname": "Noether"}
            },
            "receiverThreadIds": ["thread_child", "thread_other"],
            "senderThreadId": "thread_parent",
            "status": "inProgress",
            "tool": "spawnAgent"
        }),
    );

    assert_eq!(activity.agent_label_updates.len(), 2);
    assert!(
        activity
            .agent_label_updates
            .iter()
            .any(|update| { update.thread_id == "thread_child" && update.label == "Hooke" })
    );
    assert!(
        activity
            .agent_label_updates
            .iter()
            .any(|update| { update.thread_id == "thread_other" && update.label == "Noether" })
    );
    assert_eq!(
        activity.receiver_thread_ids,
        vec!["thread_child".to_string(), "thread_other".to_string()]
    );
    assert!(activity.collab_agent_spawn_metadata.is_none());
}

#[test]
fn tool_activity_normalizes_completed_lifecycle() {
    let activity = parse_tool_activity(
        "item/completed",
        json!({
            "id": "mcp_1",
            "type": "mcpToolCall",
            "arguments": {},
            "server": "filesystem",
            "tool": "read_file",
            "status": "completed"
        }),
    );

    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Completed);
    assert_eq!(activity.source, ToolActivitySource::McpToolCall);
    assert_eq!(activity.item_id, "mcp_1");
    assert_eq!(activity.raw_tool_name.as_deref(), Some("read_file"));
    assert_eq!(activity.raw_item_status.as_deref(), Some("completed"));
}

#[test]
fn activity_normalizes_reasoning_item_started_notifications() {
    let event = parse_turn_stream_event(
        "item/started",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "item": {
                "id": "reasoning_1",
                "type": "reasoning",
                "content": ["Raw hidden reasoning details."],
                "summary": ["Checking options."]
            }
        })),
    )
    .unwrap()
    .unwrap();

    assert!(event.tool_activity().is_none());
    let activity = event.activity().expect("expected reasoning activity");
    assert_eq!(activity.thread_id, "thread_123");
    assert_eq!(activity.turn_id, "turn_123");
    assert_eq!(activity.item_id, "reasoning_1");
    assert_eq!(activity.item_type, "reasoning");
    assert_eq!(activity.source, ToolActivitySource::Reasoning);
    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Started);
    assert_eq!(
        activity.reasoning_summary_text.as_deref(),
        Some("Checking options.")
    );
    assert_eq!(activity.reasoning_summary_delta, None);
    assert_eq!(activity.reasoning_summary_index, None);
}

#[test]
fn activity_normalizes_reasoning_item_completed_notifications() {
    let activity = parse_item_activity(
        "item/completed",
        json!({
            "id": "reasoning_1",
            "type": "reasoning",
            "content": ["Raw hidden reasoning details."],
            "summary": ["Checked options. ", "Selected the bounded path."]
        }),
    );

    assert_eq!(activity.source, ToolActivitySource::Reasoning);
    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Completed);
    assert_eq!(
        activity.reasoning_summary_text.as_deref(),
        Some("Checked options. Selected the bounded path.")
    );
    assert!(
        !activity
            .reasoning_summary_text
            .as_deref()
            .unwrap()
            .contains("Raw hidden")
    );
}

#[test]
fn activity_normalizes_reasoning_summary_text_delta_as_update() {
    let event = parse_turn_stream_event(
        "item/reasoning/summaryTextDelta",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "itemId": "reasoning_1",
            "summaryIndex": 0,
            "delta": "Checking options"
        })),
    )
    .unwrap()
    .unwrap();

    let activity = event.activity().expect("expected reasoning update");
    assert_eq!(activity.thread_id, "thread_123");
    assert_eq!(activity.turn_id, "turn_123");
    assert_eq!(activity.item_id, "reasoning_1");
    assert_eq!(activity.item_type, "reasoning");
    assert_eq!(activity.source, ToolActivitySource::Reasoning);
    assert_eq!(activity.lifecycle, ToolActivityLifecycle::Updated);
    assert_eq!(activity.reasoning_summary_index, Some(0));
    assert_eq!(
        activity.reasoning_summary_delta.as_deref(),
        Some("Checking options")
    );
}

#[test]
fn activity_does_not_expose_reasoning_text_delta() {
    let event = parse_turn_stream_event(
        "item/reasoning/textDelta",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "itemId": "reasoning_1",
            "contentIndex": 0,
            "delta": "Raw hidden reasoning details."
        })),
    )
    .unwrap()
    .unwrap();

    assert!(event.activity().is_none());
    assert!(event.tool_activity().is_none());
    let TurnStreamEvent::ReasoningTextDelta { delta, .. } = event else {
        panic!("expected raw reasoning text delta event");
    };
    assert_eq!(delta, "Raw hidden reasoning details.");
}

#[test]
fn activity_preserves_child_thread_identity_for_reasoning() {
    let event = parse_turn_stream_event(
        "item/started",
        Some(json!({
            "threadId": "thread_child",
            "turnId": "turn_child",
            "item": {
                "id": "reasoning_child",
                "type": "reasoning",
                "summary": []
            }
        })),
    )
    .unwrap()
    .unwrap();

    let activity = event.activity().expect("expected child reasoning activity");
    assert_eq!(activity.thread_id, "thread_child");
    assert_eq!(activity.turn_id, "turn_child");
    assert_eq!(activity.item_id, "reasoning_child");
    assert_eq!(activity.source, ToolActivitySource::Reasoning);
}

#[test]
fn tool_activity_ignores_non_operational_items() {
    let event = parse_turn_stream_event(
        "item/started",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "item": {
                "id": "message_1",
                "type": "agentMessage",
                "text": "hello"
            }
        })),
    )
    .unwrap()
    .unwrap();

    assert!(event.tool_activity().is_none());
}

#[test]
fn command_approval_request_preserves_diagnostic_payload() {
    let request = parse_approval_request(
        json!(77),
        "item/commandExecution/requestApproval",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "itemId": "cmd_123",
            "approvalId": null,
            "command": "Remove-Item target",
            "cwd": "C:/work/beryl",
            "reason": "requires manual approval",
            "proposedExecpolicyAmendment": ["Remove-Item"]
        })),
    )
    .unwrap();

    assert_eq!(request.method(), "item/commandExecution/requestApproval");
    assert_eq!(request.thread_id(), Some("thread_123"));
    assert_eq!(request.turn_id(), Some("turn_123"));
    assert_eq!(request.item_id(), Some("cmd_123"));
    assert_eq!(request.command(), Some("Remove-Item target"));
    assert_eq!(request.cwd(), Some("C:/work/beryl"));
    assert_eq!(request.reason(), Some("requires manual approval"));
    assert!(request.kind().denial_response_interrupts_turn());
    assert!(
        request
            .pretty_params()
            .contains("proposedExecpolicyAmendment")
    );
}

#[test]
fn dynamic_tool_call_request_preserves_call_context_and_arguments() {
    let request = parse_dynamic_tool_call_request(
        json!("request_1"),
        "item/tool/call",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "callId": "call_123",
            "namespace": "beryl",
            "tool": "apply_graph_patch",
            "arguments": {
                "ops": []
            }
        })),
    )
    .unwrap()
    .unwrap();

    assert_eq!(request.method(), "item/tool/call");
    assert_eq!(request.request_id(), &json!("request_1"));
    assert_eq!(request.thread_id(), "thread_123");
    assert_eq!(request.turn_id(), "turn_123");
    assert_eq!(request.call_id(), "call_123");
    assert_eq!(request.namespace(), Some("beryl"));
    assert_eq!(request.tool(), "apply_graph_patch");
    assert_eq!(request.arguments(), &json!({ "ops": [] }));
    assert!(request.summary().contains("apply_graph_patch"));
    assert!(request.pretty_arguments().contains("ops"));
}

#[test]
fn dynamic_tool_call_response_serializes_text_content() {
    let response = DynamicToolCallResponse::success_text("{\"ok\":true}");
    let serialized = serde_json::to_value(response).unwrap();

    assert_eq!(
        serialized,
        json!({
            "success": true,
            "contentItems": [
                {
                    "type": "inputText",
                    "text": "{\"ok\":true}"
                }
            ]
        })
    );
}

#[test]
fn dynamic_tool_call_response_serializes_image_content() {
    let response =
        DynamicToolCallResponse::failure(vec![DynamicToolCallOutputContentItem::image_url(
            "file:///tmp/graph.png",
        )]);
    let serialized = serde_json::to_value(response).unwrap();

    assert_eq!(
        serialized,
        json!({
            "success": false,
            "contentItems": [
                {
                    "type": "inputImage",
                    "imageUrl": "file:///tmp/graph.png"
                }
            ]
        })
    );
}

#[test]
fn thread_unsubscribe_response_deserializes_status() {
    let response: ThreadUnsubscribeResponse = serde_json::from_value(json!({
        "status": "unsubscribed"
    }))
    .unwrap();

    assert_eq!(response.status, ThreadUnsubscribeStatus::Unsubscribed);
}

#[test]
fn turn_start_response_deserializes_in_progress_turns() {
    let response: TurnStartResponse = serde_json::from_value(json!({
        "turn": {
            "id": "turn_123",
            "items": [],
            "status": "inProgress"
        }
    }))
    .unwrap();

    assert_eq!(response.turn.id, "turn_123");
    assert_eq!(response.turn.status, TurnStatus::InProgress);
    assert!(response.turn.items.is_empty());
}

#[test]
fn turn_steer_response_deserializes_turn_id() {
    let response: TurnSteerResponse = serde_json::from_value(json!({
        "turnId": "turn_123"
    }))
    .unwrap();

    assert_eq!(response.turn_id, "turn_123");
}

#[test]
fn active_turn_not_steerable_error_recognizes_codex_error_info() {
    let error = JsonRpcError {
        code: -32000,
        message: "active turn cannot be steered".to_string(),
        data: Some(json!({
            "codexErrorInfo": {
                "activeTurnNotSteerable": {
                    "turnKind": "compact"
                }
            }
        })),
    };

    let steering_error = active_turn_not_steerable_error(&error).unwrap();
    assert_eq!(steering_error.turn_kind, NonSteerableTurnKind::Compact);
}

#[test]
fn turn_start_options_preserve_model_and_reasoning_overrides() {
    let options = TurnStartOptions::default()
        .with_model("gpt-5.5")
        .with_reasoning_effort("high");

    assert_eq!(options.model(), Some("gpt-5.5"));
    assert_eq!(options.reasoning_effort(), Some("high"));
}

#[test]
fn turn_start_options_ignore_empty_overrides() {
    let options = TurnStartOptions::default()
        .with_model("")
        .with_reasoning_effort("");

    assert_eq!(options.model(), None);
    assert_eq!(options.reasoning_effort(), None);
}

#[test]
fn user_input_text_fragments_serialize_in_order() {
    assert_eq!(
        serde_json::to_value(vec![
            UserInput::text("First fragment"),
            UserInput::text("Second fragment")
        ])
        .unwrap(),
        json!([
            {
                "type": "text",
                "text": "First fragment"
            },
            {
                "type": "text",
                "text": "Second fragment"
            }
        ])
    );
}

#[test]
fn user_input_local_image_serializes_in_order_with_label_text() {
    assert_eq!(
        serde_json::to_value(vec![
            UserInput::text("Image A:"),
            UserInput::local_image("/tmp/beryl/image-a.png"),
            UserInput::text("Describe it")
        ])
        .unwrap(),
        json!([
            {
                "type": "text",
                "text": "Image A:"
            },
            {
                "type": "localImage",
                "path": "/tmp/beryl/image-a.png"
            },
            {
                "type": "text",
                "text": "Describe it"
            }
        ])
    );
}

#[test]
fn agent_message_phase_deserializes_final_answer() {
    let response: TurnStartResponse = serde_json::from_value(json!({
        "turn": {
            "id": "turn_123",
            "items": [
                {
                    "id": "item_1",
                    "type": "agentMessage",
                    "phase": "final_answer",
                    "text": "done"
                }
            ],
            "status": "completed"
        }
    }))
    .unwrap();

    let item = response.turn.items.first().unwrap();
    let beryl_backend::ThreadItem::AgentMessage(message) = item else {
        panic!("expected agent message item");
    };

    assert_eq!(message.phase, Some(ProtocolPhase::FinalAnswer));
    assert_eq!(response.turn.status, TurnStatus::Completed);
    assert!(matches!(
        serde_json::from_value::<ThreadStatus>(json!({
            "type": "idle"
        })),
        Ok(ThreadStatus::Idle)
    ));
}

#[test]
fn thread_history_deserializes_user_message_items() {
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
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_123",
                "items": [
                    {
                        "id": "item_user",
                        "type": "userMessage",
                        "content": [
                            {
                                "type": "text",
                                "text": "Explain the workspace"
                            }
                        ]
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let item = response.thread.turns[0].items.first().unwrap();
    let beryl_backend::ThreadItem::UserMessage(message) = item else {
        panic!("expected user message item");
    };

    assert_eq!(message.id, "item_user");
    assert_eq!(
        message.content,
        vec![UserInput::Text {
            text: "Explain the workspace".to_string()
        }]
    );
}

#[test]
fn thread_history_preserves_context_compaction_as_generic_item() {
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
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_123",
                "items": [
                    {
                        "id": "item_compact",
                        "type": "contextCompaction"
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let item = response.thread.turns[0].items.first().unwrap();
    let beryl_backend::ThreadItem::Generic(item) = item else {
        panic!("expected generic item");
    };

    assert_eq!(item.item_type, "contextCompaction");
}

#[test]
fn thread_history_deserializes_image_generation_items() {
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
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "idle"
            },
            "turns": [{
                "id": "turn_123",
                "items": [
                    {
                        "id": "image_generation_1",
                        "type": "imageGeneration",
                        "result": "iVBORw0KGgo=",
                        "revisedPrompt": "A small blue glass bird on a desk",
                        "savedPath": "C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png",
                        "status": "generating"
                    }
                ],
                "status": "completed"
            }],
            "updatedAt": 2
        }
    }))
    .unwrap();

    let item = response.thread.turns[0].items.first().unwrap();
    let ThreadItem::ImageGeneration(item) = item else {
        panic!("expected image generation item");
    };

    assert_eq!(item.id, "image_generation_1");
    assert_eq!(item.status.as_deref(), Some("generating"));
    assert_eq!(
        item.revised_prompt.as_deref(),
        Some("A small blue glass bird on a desk")
    );
    assert_eq!(item.result.as_deref(), Some("iVBORw0KGgo="));
    assert_eq!(
        item.saved_path.as_deref(),
        Some("C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png")
    );
}

#[test]
fn thread_resume_options_serialize_metadata_only_control() {
    assert_eq!(
        serde_json::to_value(ThreadResumeOptions::metadata_only()).unwrap(),
        json!({
            "excludeTurns": true
        })
    );
}

#[test]
fn thread_read_options_serialize_metadata_and_history_controls() {
    assert_eq!(
        serde_json::to_value(ThreadReadOptions::metadata_only()).unwrap(),
        json!({
            "includeTurns": false
        })
    );
    assert_eq!(
        serde_json::to_value(ThreadReadOptions::include_turns()).unwrap(),
        json!({
            "includeTurns": true
        })
    );
}

#[test]
fn thread_read_response_deserializes_thread_metadata() {
    let response: ThreadReadResponse = serde_json::from_value(json!({
        "thread": {
            "cliVersion": "0.125.0",
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": "thread_123",
            "modelProvider": "openai",
            "preview": "hello",
            "source": "appServer",
            "status": {
                "type": "notLoaded"
            },
            "turns": [],
            "updatedAt": 2
        }
    }))
    .unwrap();

    assert_eq!(response.thread.summary().id, "thread_123");
    assert!(response.thread.turns.is_empty());
}

#[test]
fn thread_turns_list_options_serialize_page_controls() {
    let options = ThreadTurnsListOptions::page(50)
        .with_cursor("turn_cursor")
        .with_sort_direction(SortDirection::Desc);

    assert_eq!(
        serde_json::to_value(options).unwrap(),
        json!({
            "cursor": "turn_cursor",
            "limit": 50,
            "sortDirection": "desc"
        })
    );
}

#[test]
fn thread_turns_list_response_deserializes_page_cursors() {
    let response: ThreadTurnsListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "turn_2",
                "status": "completed",
                "items": [
                    {
                        "id": "item_2",
                        "type": "agentMessage",
                        "text": "done"
                    }
                ]
            }
        ],
        "nextCursor": "older_turns",
        "backwardsCursor": "newer_turns"
    }))
    .unwrap();

    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].id, "turn_2");
    assert_eq!(response.next_cursor.as_deref(), Some("older_turns"));
    assert_eq!(response.backwards_cursor.as_deref(), Some("newer_turns"));
}

#[test]
fn thread_turns_list_response_deserializes_image_generation_items() {
    let response: ThreadTurnsListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "turn_2",
                "status": "completed",
                "items": [
                    {
                        "id": "image_generation_1",
                        "type": "imageGeneration",
                        "result": "iVBORw0KGgo=",
                        "revisedPrompt": "A small blue glass bird on a desk",
                        "savedPath": "C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png",
                        "status": "generating"
                    }
                ]
            }
        ]
    }))
    .unwrap();

    let item = response.data[0].items.first().unwrap();
    let ThreadItem::ImageGeneration(item) = item else {
        panic!("expected image generation item");
    };

    assert_eq!(item.id, "image_generation_1");
    assert_eq!(item.status.as_deref(), Some("generating"));
    assert_eq!(
        item.revised_prompt.as_deref(),
        Some("A small blue glass bird on a desk")
    );
    assert_eq!(item.result.as_deref(), Some("iVBORw0KGgo="));
    assert_eq!(
        item.saved_path.as_deref(),
        Some("C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png")
    );
}

#[test]
fn image_generation_stream_event_preserves_result_while_generating() {
    let event = parse_turn_stream_event(
        "item/completed",
        Some(json!({
            "threadId": "thread_123",
            "turnId": "turn_123",
            "item": {
                "id": "image_generation_1",
                "type": "imageGeneration",
                "result": "iVBORw0KGgo=",
                "revisedPrompt": "A small blue glass bird on a desk",
                "savedPath": "C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png",
                "status": "generating"
            }
        })),
    )
    .unwrap()
    .unwrap();

    let TurnStreamEvent::ItemCompleted {
        thread_id,
        turn_id,
        item,
    } = event
    else {
        panic!("expected completed image generation item");
    };
    let ThreadItem::ImageGeneration(item) = item else {
        panic!("expected image generation item");
    };

    assert_eq!(thread_id, "thread_123");
    assert_eq!(turn_id, "turn_123");
    assert_eq!(item.status.as_deref(), Some("generating"));
    assert_eq!(item.result.as_deref(), Some("iVBORw0KGgo="));
    assert_eq!(
        item.saved_path.as_deref(),
        Some("C:/Users/user/.codex/generated_images/thread_123/image_generation_1.png")
    );
}
