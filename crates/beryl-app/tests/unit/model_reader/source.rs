use super::super::qualification_support::{Fixture, ModelResponse, execute};
use super::super::*;
use beryl_model::{InputGateRevision, SyndicTurnId};
use serde_json::json;
use syndic_storage::{
    InputGateRecord, InputGateState, SyndicPointReadLimit,
    test_faults::{FixtureBatch, FixtureRecord},
};

fn gate(fixture: &Fixture, state: InputGateState) {
    gate_with_accepted(fixture, state, None);
}

fn gate_with_accepted(fixture: &Fixture, state: InputGateState, accepted: Option<u64>) {
    let old = fixture
        .storage
        .input_gate(
            fixture.home(),
            fixture.claim.thread_id(),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::InputGate(
            InputGateRecord::new(
                old.thread_id(),
                InputGateRevision::new(old.revision().get() + 1).unwrap(),
                state,
                accepted.unwrap_or(old.accepted_high_water()),
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ))
        .unwrap();
    execute(
        fixture.home(),
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(fixture.home()).unwrap(), batch),
    );
}

#[test]
fn submitted_gate_without_committed_history_does_not_follow_draft_defaults() {
    let fixture = Fixture::new();
    let initial = fixture
        .reader
        .prepare_selected(fixture.window, fixture.claim)
        .unwrap();
    assert!(initial.2);
    drop(initial);
    gate_with_accepted(
        &fixture,
        InputGateState::FinalizingHistory(SyndicTurnId::from_bytes([76; 16])),
        Some(1),
    );
    let active = fixture
        .reader
        .prepare_selected_status(fixture.window, fixture.claim)
        .unwrap();
    assert!(!active.2);
    active.0.with_current_status(|| ()).unwrap();
    drop(active);
    gate(&fixture, InputGateState::Idle);
    let settled = fixture
        .reader
        .prepare_selected(fixture.window, fixture.claim)
        .unwrap();
    assert!(!settled.2);
    assert_eq!(fixture.server.requests("config/read").len(), 1);
}

#[test]
fn ready_runtime_reads_exact_cwd_and_two_bounded_pages_without_inventory() {
    let fixture = Fixture::new();
    let query = fixture.query();
    let defaults = query.read_defaults().unwrap();
    assert_eq!(defaults.model.as_deref(), Some("actual-model"));
    assert_eq!(defaults.reasoning, None);
    fixture
        .server
        .enqueue(ModelResponse::page(64, Some("second-page")));
    let first = query.read_page(None).unwrap();
    fixture
        .server
        .enqueue(ModelResponse::page(64, Some("third-page")));
    let second = query.read_page(first.continuation()).unwrap();
    assert_eq!(first.records().len(), 64);
    assert_eq!(second.records().len(), 64);
    assert_eq!(
        first.records()[0].default_reasoning,
        Some(ModelReasoningEffort::High)
    );
    assert!(
        first.records()[0]
            .efforts
            .contains(ModelReasoningEffort::High)
    );
    assert!(
        !first.records()[0]
            .efforts
            .contains(ModelReasoningEffort::Ultra)
    );
    assert!(matches!(
        query.read_page(second.continuation()),
        Err(ModelReadError::Capacity)
    ));
    assert!(!query.has_failed_page());
    let requests = fixture.server.requests("model/list");
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["params"],
        json!({"limit":64,"includeHidden":false})
    );
    assert_eq!(
        requests[1]["params"],
        json!({"cursor":"second-page","limit":64,"includeHidden":false})
    );
    let configs = fixture.server.requests("config/read");
    assert_eq!(configs.len(), 2);
    assert!(configs.iter().all(|request| request["params"]
        == json!({"cwd":fixture.execution.root_path().as_str(),"includeLayers":false})));
    let initialization = fixture.server.requests("initialize");
    assert_eq!(initialization.len(), 4);
    assert!(
        initialization[0]["params"]["capabilities"]
            .get("optOutNotificationMethods")
            .is_none()
    );
    assert!(initialization[1..].iter().all(|request| {
        request["params"]["capabilities"]["optOutNotificationMethods"]
            .as_array()
            .is_some_and(|methods| !methods.is_empty())
    }));
    let cursor = second.continuation().unwrap().clone();
    drop(first);
    fixture.server.enqueue(ModelResponse::page(1, None));
    let third = query.read_page(Some(&cursor)).unwrap();
    assert_eq!(third.records().len(), 1);
    assert!(third.continuation().is_none());
    let retried = fixture.server.requests("model/list");
    assert_eq!(retried.len(), 3);
    assert_eq!(retried[2]["params"]["cursor"], "third-page");
}

#[test]
fn admitted_runtime_shared_by_second_root_uses_second_execution_cwd() {
    let fixture = Fixture::new();
    let query = fixture
        .reader
        .prepare(
            fixture.second_window,
            fixture.second_claim,
            fixture.second_execution.clone(),
        )
        .unwrap();
    assert_eq!(query.read_defaults().unwrap().reasoning, None);
    let configs = fixture.server.requests("config/read");
    assert_eq!(configs.len(), 2);
    assert_eq!(
        configs[0]["params"]["cwd"],
        fixture.execution.root_path().as_str()
    );
    assert_eq!(
        configs[1]["params"]["cwd"],
        fixture.second_execution.root_path().as_str()
    );
    assert_ne!(configs[0]["params"]["cwd"], configs[1]["params"]["cwd"]);
}

#[test]
fn failed_initial_and_later_pages_retry_only_the_same_query_cursor() {
    let fixture = Fixture::new();
    let query = fixture.query();
    fixture.server.enqueue(ModelResponse::Failure);
    assert!(matches!(
        query.read_page(None),
        Err(ModelReadError::Source(_))
    ));
    assert!(query.has_failed_page());
    fixture
        .server
        .enqueue(ModelResponse::page(1, Some("later-page")));
    let first = query.retry_page().unwrap();
    fixture.server.enqueue(ModelResponse::Failure);
    assert!(matches!(
        query.read_page(first.continuation()),
        Err(ModelReadError::Source(_))
    ));
    assert!(matches!(query.read_page(None), Err(ModelReadError::Retry)));
    first
        .with_current(|records| assert_eq!(records.len(), 1))
        .unwrap();
    fixture.server.enqueue(ModelResponse::page(0, None));
    let empty = query.retry_page().unwrap();
    assert!(empty.records().is_empty());
    assert!(empty.continuation().is_none());
    assert!(matches!(query.retry_page(), Err(ModelReadError::Retry)));
    let requests = fixture.server.requests("model/list");
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0]["params"], requests[1]["params"]);
    assert_eq!(requests[2]["params"], requests[3]["params"]);
    assert_eq!(requests[2]["params"]["cursor"], "later-page");
}

#[test]
fn foreign_continuation_never_dispatches_and_empty_page_is_current() {
    let fixture = Fixture::new();
    let (query, page) = fixture.first_page(0, Some("cursor"));
    page.with_current(|records| assert!(records.is_empty()))
        .unwrap();
    let other = fixture.query();
    assert!(!page.belongs_to(&other));
    assert!(page.belongs_to(&query));
    assert!(matches!(
        other.read_page(page.continuation()),
        Err(ModelReadError::Continuation)
    ));
    assert_eq!(fixture.server.requests("model/list").len(), 1);
}

#[test]
fn unrelated_home_write_revalidation_keeps_exact_page_without_backend_reread() {
    let fixture = Fixture::new();
    let (query, page) = fixture.first_page(1, None);
    fixture.unrelated_write();
    let mut elected = false;
    assert!(page.with_current(|_| elected = true).is_err());
    assert!(!elected);
    query.revalidate().unwrap();
    page.with_current(|records| assert_eq!(records[0].id, "id-0"))
        .unwrap();
    assert_eq!(fixture.server.requests("model/list").len(), 1);
}

#[test]
fn changed_claim_and_retired_readiness_cannot_revive_original_page() {
    for change_claim in [true, false] {
        let fixture = Fixture::new();
        let (query, page) = fixture.first_page(1, None);
        if change_claim {
            fixture.replace_claim();
        } else {
            fixture
                .service()
                .retire_model_readiness_for_test(fixture.execution.runtime_id());
        }
        assert!(query.revalidate().is_err());
        let mut elected = false;
        assert!(page.with_current(|_| elected = true).is_err());
        assert!(!elected);
        assert!(query.read_page(None).is_err());
        assert_eq!(fixture.server.requests("model/list").len(), 1);
    }
}

#[test]
fn genuine_runtime_retry_cannot_revive_the_original_ready_query_or_page() {
    let mut fixture = Fixture::new();
    let (original, page) = fixture.first_page(1, None);
    fixture
        .service()
        .retire_model_readiness_for_test(fixture.execution.runtime_id());
    assert!(original.revalidate().is_err());
    fixture.retry_runtime();
    let successor = fixture.query();
    successor.with_current(|| ()).unwrap();
    assert_eq!(
        successor.read_defaults().unwrap().model.as_deref(),
        Some("actual-model")
    );
    assert!(original.revalidate().is_err());
    let mut elected = false;
    assert!(page.with_current(|_| elected = true).is_err());
    assert!(!elected);
    assert!(original.read_page(None).is_err());
    assert_eq!(fixture.server.requests("model/list").len(), 1);
    assert_eq!(fixture.server.requests("initialize").len(), 4);
}

#[test]
fn preparation_fence_survives_query_capacity_failure_without_a_new_ready_scope() {
    let fixture = Fixture::new();
    let queries = (0..4).map(|_| fixture.query()).collect::<Vec<_>>();
    let fence = fixture
        .reader
        .preparation_fence(fixture.window, fixture.claim)
        .unwrap();
    assert!(fence.matches_selection(fixture.window, fixture.claim));
    assert!(!fence.matches_selection(fixture.second_window, fixture.second_claim));
    assert!(!fence.matches_selection(fixture.window, fixture.second_claim));
    assert!(matches!(
        fixture.reader.prepare_fenced(&fence),
        Err(ModelReadError::Capacity)
    ));
    assert!(!fence.belongs_to_query(&queries[0]));
    assert!(fixture.server.requests("model/list").is_empty());
    drop(queries);
    let (query, execution, draft_only) = fixture.reader.prepare_fenced(&fence).unwrap();
    assert!(fence.belongs_to_query(&query));
    assert_eq!(execution, fixture.execution);
    assert!(draft_only);
    fence.with_current(|| ()).unwrap();
    query.with_current(|| ()).unwrap();
}

#[test]
fn failed_default_preparation_retry_keeps_the_original_attempt_fence() {
    let mut fixture = Fixture::new();
    let fence = fixture
        .reader
        .preparation_fence(fixture.window, fixture.claim)
        .unwrap();
    let (query, _, _) = fixture.reader.prepare_fenced(&fence).unwrap();
    fixture.server.fail_next_config_read();
    assert!(query.read_defaults().is_err());
    drop(query);
    fixture.unrelated_write();
    fence.revalidate().unwrap();
    let (retried, _, _) = fixture.reader.prepare_fenced(&fence).unwrap();
    assert!(fence.belongs_to_query(&retried));
    assert_eq!(
        retried.read_defaults().unwrap().model.as_deref(),
        Some("actual-model")
    );
    drop(retried);
    fixture
        .service()
        .retire_model_readiness_for_test(fixture.execution.runtime_id());
    fixture.retry_runtime();
    assert!(fence.revalidate().is_err());
    assert!(fixture.reader.prepare_fenced(&fence).is_err());
    let successor = fixture
        .reader
        .preparation_fence(fixture.window, fixture.claim)
        .unwrap();
    let (query, _, _) = fixture.reader.prepare_fenced(&successor).unwrap();
    query.with_current(|| ()).unwrap();
    assert!(!fence.belongs_to_query(&query));
    assert_eq!(
        query.read_defaults().unwrap().model.as_deref(),
        Some("actual-model")
    );
    assert!(fixture.server.requests("model/list").is_empty());
}

#[test]
fn active_then_idle_gate_revision_rejects_old_popup_but_status_stays_observable() {
    let fixture = Fixture::new();
    let (query, page) = fixture.first_page(1, None);
    gate(
        &fixture,
        InputGateState::FinalizingHistory(SyndicTurnId::from_bytes([75; 16])),
    );
    assert!(matches!(
        query.revalidate(),
        Err(ModelReadError::Source(ModelSourceError::Active))
    ));
    let status = fixture
        .reader
        .prepare_status(fixture.window, fixture.claim, fixture.execution.clone())
        .unwrap();
    status.with_current_status(|| ()).unwrap();
    assert!(matches!(
        status.read_page(None),
        Err(ModelReadError::Source(ModelSourceError::Active))
    ));
    gate(&fixture, InputGateState::Idle);
    assert!(query.revalidate().is_err());
    let mut elected = false;
    assert!(page.with_current(|_| elected = true).is_err());
    assert!(!elected);
    let successor = fixture.query();
    successor.with_current(|| ()).unwrap();
}

#[test]
fn closed_home_and_publication_reject_retained_queries_and_pages() {
    let mut fixture = Fixture::new();
    let (query, page) = fixture.first_page(1, None);
    fixture.close_service();
    let mut elected = false;
    assert!(page.with_current(|_| elected = true).is_err());
    assert!(!elected);
    assert!(query.revalidate().is_err());
    drop(page);
    drop(query);
    assert_eq!(fixture.reader.capacity.queries.load(Ordering::Acquire), 0);
    assert_eq!(fixture.reader.capacity.pages.load(Ordering::Acquire), 0);
}

#[test]
fn retired_publication_never_elects_or_dispatches_a_retained_page() {
    let mut fixture = Fixture::new();
    let (query, page) = fixture.first_page(1, None);
    let original = std::mem::replace(&mut fixture.lifetime, Arc::new(()));
    drop(original);
    let mut elected = false;
    assert!(matches!(
        page.with_current(|_| elected = true),
        Err(ModelReadError::Closed)
    ));
    assert!(!elected);
    assert!(matches!(query.read_page(None), Err(ModelReadError::Closed)));
    assert_eq!(fixture.server.requests("model/list").len(), 1);
}

#[test]
fn duplicate_pending_and_close_inflight_drain_capacity_after_exact_completion() {
    let fixture = Fixture::new();
    let query = fixture.query();
    let held = fixture.server.hold(ModelResponse::page(1, None));
    let worker = Arc::clone(&query);
    let task = std::thread::spawn(move || worker.read_page(None));
    held.wait();
    assert!(matches!(
        query.read_page(None),
        Err(ModelReadError::Pending)
    ));
    assert_eq!(fixture.server.requests("model/list").len(), 1);
    assert_eq!(fixture.reader.capacity.pages.load(Ordering::Acquire), 1);
    query.close();
    held.release();
    assert!(matches!(task.join().unwrap(), Err(ModelReadError::Closed)));
    assert_eq!(fixture.reader.capacity.pages.load(Ordering::Acquire), 0);
    assert_eq!(fixture.reader.capacity.queries.load(Ordering::Acquire), 1);
    drop(query);
    assert_eq!(fixture.reader.capacity.queries.load(Ordering::Acquire), 0);
}

#[test]
fn timed_out_page_drains_request_capacity_and_retains_exact_retry() {
    let fixture = Fixture::with_limits(3, Duration::from_millis(200));
    let query = fixture.query();
    let held = fixture.server.hold(ModelResponse::page(1, None));
    let worker = Arc::clone(&query);
    let task = std::thread::spawn(move || worker.read_page(None));
    held.wait();
    assert!(matches!(
        task.join().unwrap(),
        Err(ModelReadError::Source(_))
    ));
    held.release();
    assert_eq!(fixture.reader.capacity.pages.load(Ordering::Acquire), 0);
    fixture.server.enqueue(ModelResponse::page(1, None));
    let page = query.retry_page().unwrap();
    assert_eq!(page.records().len(), 1);
    assert_eq!(fixture.server.requests("model/list").len(), 2);
}
