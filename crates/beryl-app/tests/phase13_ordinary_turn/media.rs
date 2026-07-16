use beryl_app::cas_projection::{CasProjectionCoordinator, OrdinaryTurnExecutionOutcome};
use serde_json::json;
use syndic_storage::{
    CanonicalItemKind, CanonicalItemPayload, GeneratedMediaResourceDisposition, ProviderItemKind,
    ProviderItemLifecycle, ResourceBacking, TurnLifecycle, TurnTerminalOutcome,
};

use crate::{
    backend::{FakeAppServer, ProjectionStep, TurnStartAction, TurnStartReply},
    support::{NoTools, execution_request, item_by_kind, obtain, process},
    syndic::{Fixture, execution_binding, point_limit},
};

const INPUT: &str = "phase13 generated media input";
const CAS_THREAD: &str = "phase13-media-thread";
const CAS_TURN: &str = "phase13-media-turn";
const USER_ITEM: &str = "phase13-media-user";
const IMAGE_ITEM: &str = "phase13-media-image";
const IMAGE_VIEW_ITEM: &str = "phase13-media-image-view";
const SUBAGENT_ITEM: &str = "phase13-media-subagent";

#[test]
fn pending_generated_asset_preserves_provider_completion_without_finalizing_history() {
    let mut fixture = Fixture::new(83);
    let submitted = fixture.submit_text(INPUT);
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![
                turn_started(),
                item_notification("item/started", user_item()),
                item_notification("item/completed", user_item()),
                item_notification("item/started", image_item("inProgress")),
                item_notification("item/completed", image_item("completed")),
                notify(
                    "turn/completed",
                    json!({
                        "threadId": CAS_THREAD,
                        "turn": { "id": CAS_TURN, "status": "completed" }
                    }),
                ),
            ],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(83));
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
    drop(projection);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().item_count(), 2);
    assert_eq!(state.record().finalized_item_count(), 1);

    let image_id = item_by_kind(&fixture, submitted.turn, CanonicalItemKind::GeneratedMedia);
    let image = fixture
        .storage
        .canonical_item(&fixture.store, image_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        image.record().provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    let CanonicalItemPayload::GeneratedMedia(resource_id) = image.record().payload() else {
        panic!("generated-media item must select its resource")
    };
    let resource = fixture
        .storage
        .resource(&fixture.store, *resource_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(resource.record().item_id(), image_id);
    assert_eq!(
        resource.record().backing(),
        ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
    );
    assert!(resource.record().projection_id().is_none());
    assert!(resource.record().ordinal().is_none());
    assert!(resource.record().media_type().is_none());
    assert!(resource.record().byte_length().is_none());
    assert!(resource.record().digest().is_none());
    let summary = fixture
        .storage
        .history_summary(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(!summary.record().complete());
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn activity_only_and_completion_only_items_finalize_without_text_or_resource_payloads() {
    let mut fixture = Fixture::new(84);
    let submitted = fixture.submit_text(INPUT);
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![
                turn_started(),
                item_notification("item/started", user_item()),
                item_notification("item/completed", user_item()),
                item_notification("item/started", image_view_item()),
                item_notification("item/completed", image_view_item()),
                item_notification("item/completed", subagent_item()),
                notify(
                    "turn/completed",
                    json!({
                        "threadId": CAS_THREAD,
                        "turn": { "id": CAS_TURN, "status": "completed" }
                    }),
                ),
            ],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(84));
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
    drop(projection);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().item_count(), 3);
    assert_eq!(state.record().finalized_item_count(), 3);
    assert_activity_payload(
        &fixture,
        submitted.turn,
        CanonicalItemKind::Activity(ProviderItemKind::ImageView),
    );
    assert_activity_payload(
        &fixture,
        submitted.turn,
        CanonicalItemKind::Activity(ProviderItemKind::SubAgentActivity),
    );
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

fn assert_activity_payload(
    fixture: &Fixture,
    turn: beryl_model::SyndicTurnId,
    kind: CanonicalItemKind,
) {
    let item_id = item_by_kind(fixture, turn, kind);
    let item = fixture
        .storage
        .canonical_item(&fixture.store, item_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        item.record().provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    assert_eq!(item.record().payload(), &CanonicalItemPayload::Activity);
    assert!(item.record().payload().content().is_none());
}

fn item_notification(method: &'static str, item: serde_json::Value) -> TurnStartAction {
    notify(
        method,
        json!({
            "threadId": CAS_THREAD,
            "turnId": CAS_TURN,
            "item": item
        }),
    )
}

fn notify(method: &'static str, params: serde_json::Value) -> TurnStartAction {
    TurnStartAction::notification(method, params)
}

fn turn_started() -> TurnStartAction {
    notify(
        "turn/started",
        json!({
            "threadId": CAS_THREAD,
            "turn": { "id": CAS_TURN, "status": "inProgress" }
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

fn image_item(status: &str) -> serde_json::Value {
    json!({
        "id": IMAGE_ITEM,
        "type": "imageGeneration",
        "status": status,
        "revisedPrompt": "a path-free generated fixture",
        "result": "provider-owned-result-reference",
        "savedPath": "C:/provider/private/generated.png"
    })
}

fn image_view_item() -> serde_json::Value {
    json!({
        "id": IMAGE_VIEW_ITEM,
        "type": "imageView",
        "path": "C:/provider/private/viewed.png"
    })
}

fn subagent_item() -> serde_json::Value {
    json!({
        "id": SUBAGENT_ITEM,
        "type": "subAgentActivity",
        "kind": "started",
        "agentThreadId": "phase13-media-child-thread",
        "agentPath": "provider/private/agent/path"
    })
}
