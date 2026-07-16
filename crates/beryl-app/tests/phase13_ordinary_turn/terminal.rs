use beryl_app::cas_projection::{CasProjectionCoordinator, OrdinaryTurnExecutionOutcome};
use beryl_home_store::CursorReadLimits;
use serde_json::json;
use syndic_storage::{
    AssistantMessagePhase, CanonicalItemKind, ContentLifecycle, ProviderItemKind,
    ProviderItemLifecycle, SourceEventPayload, TurnIncompleteReason, TurnLifecycle,
    TurnTerminalOutcome,
};

use crate::{
    backend::{EXECUTION_ROOT, FakeAppServer, ProjectionStep, TurnStartAction, TurnStartReply},
    support::{
        NoTools, RecordingTools, execution_request, item_by_kind, item_text, obtain, process,
        source_events,
    },
    syndic::{Fixture, execution_binding, point_limit},
};

const INPUT: &str = "phase13 ordinary input";
const CAS_THREAD: &str = "phase13-terminal-thread";
const CAS_TURN: &str = "phase13-terminal-turn";
const USER_ITEM: &str = "phase13-user-item";
const ASSISTANT_ITEM: &str = "phase13-assistant-item";
const COMMAND_ITEM: &str = "phase13-command-item";

include!("terminal_stream.rs");

#[test]
fn buffered_terminal_stream_preserves_bounded_text_and_terminal_suffix() {
    let mut fixture = Fixture::new(60);
    let submitted = fixture.submit_text(INPUT);
    let first = "a".repeat(32 * 1024);
    let second = "\u{03a9}".repeat(16 * 1024);
    let third = "b".repeat(32 * 1024);
    let fourth = "\u{00e9}".repeat(16 * 1024);
    let suffix = "Ω-terminal-suffix";
    let assistant_text = format!("{first}{second}{third}{fourth}{suffix}");
    let command_prefix = "bounded ";
    let command_delta = "command output\n";
    let command_output = format!("{command_prefix}{command_delta}");
    let before_reply = vec![
        notify(
            "turn/started",
            json!({
                "threadId": CAS_THREAD,
                "turn": turn(CAS_TURN, "inProgress")
            }),
        ),
        user_started(),
        user_completed(),
        notify(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(ASSISTANT_ITEM, "")
            }),
        ),
        agent_delta(&first),
        agent_delta(&second),
        agent_delta(&third),
        agent_delta(&fourth),
        notify(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": command_item(COMMAND_ITEM, "inProgress", Some(command_prefix))
            }),
        ),
        notify(
            "item/commandExecution/outputDelta",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "itemId": COMMAND_ITEM,
                "delta": command_delta
            }),
        ),
        notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": command_item(COMMAND_ITEM, "completed", Some(&command_output))
            }),
        ),
        notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(ASSISTANT_ITEM, &assistant_text)
            }),
        ),
        notify(
            "turn/completed",
            json!({
                "threadId": CAS_THREAD,
                "turn": turn(CAS_TURN, "completed")
            }),
        ),
    ];
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply,
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(60));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("expected terminal ordinary execution, got {outcome:?}")
    };
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(status.incomplete_reason(), None);
    assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD);
    drop(projection);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().item_count(), 3);
    assert_eq!(state.record().finalized_item_count(), 3);

    let assistant = item_by_kind(
        &fixture,
        submitted.turn,
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer),
    );
    assert_eq!(item_text(&fixture, assistant), assistant_text);
    let operational = item_by_kind(
        &fixture,
        submitted.turn,
        CanonicalItemKind::Operational(ProviderItemKind::CommandExecution),
    );
    assert_eq!(item_text(&fixture, operational), command_output);
    let assistant_record = fixture
        .storage
        .canonical_item(&fixture.store, assistant, point_limit())
        .unwrap()
        .unwrap();
    let manifest = fixture
        .storage
        .content_manifest(
            &fixture.store,
            assistant_record
                .record()
                .payload()
                .content()
                .expect("assistant fixture must own content")
                .id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Finalized);

    let assistant_delta_sizes = source_events(&fixture, submitted.turn)
        .into_iter()
        .filter_map(|event| match event.payload() {
            SourceEventPayload::ItemDelta {
                cas_item_id, text, ..
            } if cas_item_id.as_str() == ASSISTANT_ITEM => Some(text.as_str().len()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        assistant_delta_sizes,
        vec![64 * 1024, 64 * 1024, suffix.len()]
    );
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn late_mismatch_preserves_the_prefix_and_marks_terminal_history_incomplete() {
    let mut fixture = Fixture::new(62);
    let submitted = fixture.submit_text(INPUT);
    let first = "a".repeat(64 * 1024);
    let second = "b".repeat(32 * 1024);
    let durable_prefix = format!("{first}{second}");
    let mut conflicting = durable_prefix.clone();
    conflicting.replace_range(80 * 1024..80 * 1024 + 1, "c");
    let before_reply = vec![
        notify(
            "turn/started",
            json!({
                "threadId": CAS_THREAD,
                "turn": turn(CAS_TURN, "inProgress")
            }),
        ),
        user_started(),
        user_completed(),
        notify(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(ASSISTANT_ITEM, "")
            }),
        ),
        agent_delta(&first),
        agent_delta(&second),
        notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(ASSISTANT_ITEM, &conflicting)
            }),
        ),
        notify(
            "turn/completed",
            json!({
                "threadId": CAS_THREAD,
                "turn": turn(CAS_TURN, "completed")
            }),
        ),
    ];
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply,
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(62));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("expected terminal ordinary execution, got {outcome:?}")
    };
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(
        status.incomplete_reason(),
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    drop(projection);
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(
        state.record().incomplete_reason(),
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    let assistant = item_by_kind(
        &fixture,
        submitted.turn,
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer),
    );
    assert_eq!(item_text(&fixture, assistant), durable_prefix);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn terminal_audit_preserves_provider_completion_with_a_late_open_item() {
    const PROVIDER_ITEM_COUNT: u64 = 64;

    let mut fixture = Fixture::new(64);
    let submitted = fixture.submit_text(INPUT);
    let mut after_reply = vec![turn_started(), user_started(), user_completed()];
    for index in 0..PROVIDER_ITEM_COUNT {
        let item_id = format!("phase13-audit-item-{index}");
        after_reply.push(notify(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(&item_id, "")
            }),
        ));
        if index + 1 < PROVIDER_ITEM_COUNT {
            after_reply.push(notify(
                "item/completed",
                json!({
                    "threadId": CAS_THREAD,
                    "turnId": CAS_TURN,
                    "item": agent_item(&item_id, "")
                }),
            ));
        }
    }
    after_reply.push(notify(
        "turn/completed",
        json!({
            "threadId": CAS_THREAD,
            "turn": turn(CAS_TURN, "completed")
        }),
    ));
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(64));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("expected terminal ordinary execution, got {outcome:?}")
    };
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(
        status.incomplete_reason(),
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    drop(projection);
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().item_count(), PROVIDER_ITEM_COUNT + 1);
    assert_eq!(state.record().finalized_item_count(), PROVIDER_ITEM_COUNT);
    let first_page = fixture
        .storage
        .turn_items(
            &fixture.store,
            submitted.turn,
            None,
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(first_page.records().len(), 64);
    assert!(first_page.has_more());
    let second_page = fixture
        .storage
        .turn_items(
            &fixture.store,
            submitted.turn,
            Some(first_page.records().last().unwrap().ordinal()),
            CursorReadLimits::new(64, 64 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(second_page.records().len(), 1);
    assert!(!second_page.has_more());
    let omitted = fixture
        .storage
        .canonical_item(
            &fixture.store,
            second_page.records()[0].item_id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    let manifest = fixture
        .storage
        .content_manifest(
            &fixture.store,
            omitted
                .record()
                .payload()
                .content()
                .expect("open assistant fixture must own content")
                .id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Live);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

fn notify(method: &'static str, params: serde_json::Value) -> TurnStartAction {
    TurnStartAction::notification(method, params)
}

fn user_started() -> TurnStartAction {
    user_notification("item/started")
}

fn turn_started() -> TurnStartAction {
    notify(
        "turn/started",
        json!({
            "threadId": CAS_THREAD,
            "turn": turn(CAS_TURN, "inProgress")
        }),
    )
}

fn user_completed() -> TurnStartAction {
    user_notification("item/completed")
}

fn user_notification(method: &'static str) -> TurnStartAction {
    notify(
        method,
        json!({
            "threadId": CAS_THREAD,
            "turnId": CAS_TURN,
            "item": user_item()
        }),
    )
}

fn user_item() -> serde_json::Value {
    json!({
        "id": USER_ITEM,
        "type": "userMessage",
        "clientId": null,
        "content": [{ "type": "text", "text": INPUT }]
    })
}

fn agent_delta(delta: &str) -> TurnStartAction {
    notify(
        "item/agentMessage/delta",
        json!({
            "threadId": CAS_THREAD,
            "turnId": CAS_TURN,
            "itemId": ASSISTANT_ITEM,
            "delta": delta
        }),
    )
}

fn agent_item(id: &str, text: &str) -> serde_json::Value {
    json!({
        "id": id,
        "type": "agentMessage",
        "phase": "final_answer",
        "text": text
    })
}

fn command_item(id: &str, status: &str, output: Option<&str>) -> serde_json::Value {
    let mut item = json!({
        "id": id,
        "type": "commandExecution",
        "command": "Write-Output phase13",
        "commandActions": [],
        "cwd": EXECUTION_ROOT,
        "status": status
    });
    if let Some(output) = output {
        item["aggregatedOutput"] = json!(output);
    }
    item
}

fn turn(id: &str, status: &str) -> serde_json::Value {
    json!({ "id": id, "status": status })
}
