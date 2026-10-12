use super::*;
use beryl_home_store::CursorReadLimits;
use syndic_storage::{
    ProjectionLifecycle, SyndicPointReadLimit, TranscriptViewEntryRecord, TurnLifecycle,
};

fn code_entry(fixture: &Fixture) -> Result<Option<TranscriptViewEntryRecord>, String> {
    let home = fixture.home();
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let Some(head) = fixture
        .storage
        .transcript_view_head(&home, fixture.thread, limit)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    if head.lifecycle() != ProjectionLifecycle::Current {
        return Ok(None);
    }
    let page = fixture
        .storage
        .transcript_entries(
            &home,
            fixture.thread,
            head.generation(),
            None,
            CursorReadLimits::new(16, 65_536).unwrap(),
        )
        .map_err(|error| error.to_string())?;
    assert!(!page.has_more());
    for entry in page.records() {
        let projection = fixture
            .storage
            .projection(&home, entry.projection_id(), limit)
            .map_err(|error| error.to_string())?
            .unwrap();
        if projection.payload().resource_id().is_some() {
            return Ok(Some(entry.clone()));
        }
    }
    Ok(None)
}

fn wait_for_code(
    fixture: &Fixture,
    previous: Option<&TranscriptViewEntryRecord>,
) -> Result<TranscriptViewEntryRecord, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last_error = String::new();
    while std::time::Instant::now() < deadline {
        match code_entry(fixture) {
            Ok(Some(entry))
                if previous
                    .is_none_or(|previous| entry.item_revision() > previous.item_revision()) =>
            {
                return Ok(entry);
            }
            Ok(_) => {}
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(format!(
        "unfinished saved Code was not published: {last_error}"
    ))
}

fn code_range(
    fixture: &Fixture,
    entry: &TranscriptViewEntryRecord,
) -> syndic_storage::ProjectionSourceRange {
    fixture
        .storage
        .projection(
            &fixture.home(),
            entry.projection_id(),
            syndic::point_limit(),
        )
        .unwrap()
        .unwrap()
        .payload()
        .source_range()
        .unwrap()
}

#[test]
fn ordinary_driver_publishes_and_grows_saved_code_before_provider_completion() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(143);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_controlled_connection_loss();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &connector,
            execution_binding(),
            CasProcessGeneration::new(37_143).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = beryl_app::cas_projection::CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_000),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let home = fixture.home();
    let initial = format!("```rust\n{}", "let value = 1;\n".repeat(8_000));
    let delta = "let answer = 42;\n".repeat(1_000);
    let (outcome, observed) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut lifecycle = NoopLifecycle::default();
            let mut branch = NoopBranch::default();
            coordinator.execute_ordinary_turn(
                &home,
                &fixture.storage,
                &fixture.state.assets(),
                None,
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
        });
        server.send_observation(
            serde_json::json!({"method":"item/started","params":{
                "item":{"type":"agentMessage","id":"streaming-code","text":initial},
                "threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID,"startedAtMs":37012
            }})
            .to_string(),
        );
        let observed = (|| {
            let first = wait_for_code(&fixture, None)?;
            let state = fixture
                .storage
                .turn_state(&home, submitted.turn, syndic::point_limit())
                .unwrap()
                .unwrap();
            assert_eq!(state.lifecycle(), TurnLifecycle::Active);
            assert_eq!(state.finalized_item_count(), 0);
            assert!(
                !fixture
                    .storage
                    .history_summary(&home, fixture.thread, syndic::point_limit())
                    .unwrap()
                    .unwrap()
                    .complete()
            );
            server.send_observation(serde_json::json!({"method":"item/agentMessage/delta","params":{
                "itemId":"streaming-code","delta":delta,"threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID
            }}).to_string());
            let grown = wait_for_code(&fixture, Some(&first))?;
            assert_eq!(grown.item_id(), first.item_id());
            let first_range = code_range(&fixture, &first);
            let grown_range = code_range(&fixture, &grown);
            assert_eq!(grown_range.start(), first_range.start());
            assert_eq!(grown_range.len(), first_range.len() + delta.len() as u64);
            let state = fixture
                .storage
                .turn_state(&home, submitted.turn, syndic::point_limit())
                .unwrap()
                .unwrap();
            assert_eq!(state.lifecycle(), TurnLifecycle::Active);
            assert_eq!(state.finalized_item_count(), 0);
            Ok::<_, String>(grown)
        })();
        if observed.is_ok() {
            server.send_observation(serde_json::json!({"method":"item/completed","params":{
                "item":{"type":"agentMessage","id":"streaming-code","text":format!("{initial}{delta}")},
                "threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID,"completedAtMs":37013
            }}).to_string());
            let mut terminal: serde_json::Value =
                serde_json::from_str(&server::terminal_wire()).unwrap();
            terminal["params"]["turn"]["completedAt"] = serde_json::json!(37014);
            terminal["params"]["turn"]["durationMs"] = serde_json::json!(4);
            server.send_observation(terminal.to_string());
        } else {
            server.close_connection();
        }
        (worker.join().unwrap(), observed)
    });
    if observed.is_ok() {
        server.close_connection();
    }
    session.invalidate_connection();
    server.join();
    let grown = observed.unwrap();
    assert!(
        outcome.is_ok(),
        "ordinary terminal handoff failed: {outcome:?}"
    );
    assert_eq!(
        code_entry(&fixture).unwrap().unwrap().projection_id(),
        grown.projection_id()
    );
    assert!(
        fixture
            .storage
            .history_summary(&home, fixture.thread, syndic::point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    home.scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    drop(outcome);
    drop(home);
    drop(session);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    drop(directory);
}
