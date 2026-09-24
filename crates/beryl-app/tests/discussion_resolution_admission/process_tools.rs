use super::*;
use beryl_app::lifecycle_attention::ProcessLifecycleAttentionPool;
use beryl_backend::{DynamicToolCallOutputContentItem, DynamicToolCallResponse};
use std::sync::Arc;

fn body(response: DynamicToolCallResponse, success: bool) -> serde_json::Value {
    assert_eq!(response.success, success);
    assert_eq!(response.content_items.len(), 1);
    let DynamicToolCallOutputContentItem::InputText { text } = &response.content_items[0] else {
        panic!("text response")
    };
    assert!(text.len() < 512);
    serde_json::from_str(text).unwrap()
}

fn tools(fixture: &Fixture) -> ProcessOrdinaryDynamicToolAuthority {
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let tools = fixture.service.ordinary_dynamic_tool_authority(&pool);
    fixture
        .service
        .test_configure_discussion_resolution(fixture.settlement.clone())
        .unwrap();
    tools
}

#[test]
fn cloned_tools_admit_once_and_preserve_original_payload() {
    let fixture = Fixture::new();
    let mut tools = tools(&fixture);
    let mut clone = tools.clone();
    let first = body(
        tools.test_resolve(fixture.context("exact"), "original".into()),
        true,
    );
    assert_eq!(first["status"], "admitted");
    assert_eq!(
        body(
            clone.test_resolve(fixture.context("exact"), "replacement".into()),
            true
        ),
        first
    );
    let duplicate = body(
        clone.test_resolve(fixture.context("different"), "replacement".into()),
        true,
    );
    assert_eq!(duplicate["status"], "already_admitted");
    assert_eq!(duplicate["job_id"], first["job_id"]);
    let command = fixture.service.live_home_command().unwrap();
    let job = fixture
        .state
        .durable_jobs()
        .latest_attempt(command.home(), id(36))
        .unwrap()
        .unwrap();
    let stored = fixture
        .state
        .durable_jobs()
        .job(command.home(), job.job_id())
        .unwrap()
        .unwrap();
    assert_eq!(stored.resolution().as_str(), "original");
    drop(command);
    let late = fixture.context("late");
    fixture.close();
    assert_eq!(
        body(clone.test_resolve(late, "late".into()), false)["status"],
        "unavailable"
    );
}

#[test]
fn queued_input_returns_structured_deferral_without_admission() {
    let fixture = Fixture::new();
    let mut tools = tools(&fixture);
    let command = fixture.service.live_home_command().unwrap();
    queue_future_input(
        command.home(),
        &fixture.syndic,
        fixture.source.resolving_target.pending().active_turn_id(),
    );
    drop(command);
    let result = body(
        tools.test_resolve(fixture.context("defer"), "defer".into()),
        true,
    );
    assert_eq!(result["status"], "deferred_queued_input");
    assert!(
        result["guidance"]
            .as_str()
            .unwrap()
            .contains("later resolution tool call")
    );
    let command = fixture.service.live_home_command().unwrap();
    assert!(
        fixture
            .state
            .durable_jobs()
            .latest_attempt(command.home(), id(36))
            .unwrap()
            .is_none()
    );
    drop(command);
    fixture.close();
}

#[test]
fn foreign_generation_and_unconfigured_tools_cannot_admit() {
    let fixture = Fixture::new();
    let other = Fixture::new();
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut tools = fixture.service.ordinary_dynamic_tool_authority(&pool);
    assert_eq!(
        body(
            tools.test_resolve(fixture.context("early"), "early".into()),
            false
        )["status"],
        "unavailable"
    );
    fixture
        .service
        .test_configure_discussion_resolution(fixture.settlement.clone())
        .unwrap();
    assert_eq!(
        body(
            tools.test_resolve(other.context("foreign"), "foreign".into()),
            false
        )["status"],
        "unavailable"
    );
    other.close();
    fixture.close();
}

#[test]
fn uncertainty_keeps_process_custody_after_the_response_is_dropped() {
    let fixture = Fixture::new();
    let mut tools = tools(&fixture);
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        body(
            tools.test_resolve(fixture.context("uncertain"), "exact".into()),
            false
        )["status"],
        "outcome_unresolved"
    );
    assert_eq!(
        body(
            tools.test_resolve(fixture.context("uncertain"), "replacement".into()),
            false
        )["status"],
        "unavailable"
    );
    let command = fixture.service.live_home_command().unwrap();
    let job = fixture
        .state
        .durable_jobs()
        .latest_attempt(command.home(), id(36))
        .unwrap()
        .unwrap();
    let audit = fixture
        .operations
        .retained_audit(job.job_id())
        .expect("response disposal retains custody");
    assert!(matches!(
        audit
            .reconcile(command.home(), &fixture.syndic, &fixture.state)
            .unwrap(),
        DiscussionSettlementAuditOutcome::Settled(_)
    ));
    drop((audit, command));
    assert_eq!(
        body(
            tools.test_resolve(fixture.context("uncertain"), "replacement".into()),
            true
        )["status"],
        "admitted"
    );
    fixture.close();
}
