#[test]
fn submitted_user_correlation_uses_exact_ownerless_sealed_content() {
    let mut fixture = Fixture::new(59);
    let submitted = fixture.submit_text(INPUT);
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
    let mut session = server.admit(execution_binding().runtime_id(), process(59));
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
    assert_eq!(state.record().item_count(), 1);
    assert_eq!(state.record().finalized_item_count(), 1);
    let user = item_by_kind(&fixture, submitted.turn, CanonicalItemKind::UserInput);
    let user = fixture
        .storage
        .canonical_item(&fixture.store, user, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        user.record().provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    let content = user.record().payload().content().unwrap();
    let manifest = fixture
        .storage
        .content_manifest(&fixture.store, content.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Sealed);
    assert_eq!(manifest.record().owner(), None);

    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn a_repeated_completion_cannot_change_canonical_kind_and_marks_history_incomplete() {
    let mut fixture = Fixture::new(69);
    let submitted = fixture.submit_text(INPUT);
    let text = "provider assistant text";
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
        notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": agent_item(ASSISTANT_ITEM, text)
            }),
        ),
        notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": command_item(ASSISTANT_ITEM, "completed", Some(text))
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
    let mut session = server.admit(execution_binding().runtime_id(), process(69));
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
    assert_eq!(item_text(&fixture, assistant), text);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn dynamic_tool_request_is_answered_by_its_exact_ordinary_target() {
    let mut fixture = Fixture::new(61);
    let submitted = fixture.submit_text(INPUT);
    let expected_response = json!({
        "contentItems": [{ "type": "inputText", "text": "phase13 tool response" }],
        "success": true
    });
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![
                notify(
                    "turn/started",
                    json!({
                        "threadId": CAS_THREAD,
                        "turn": turn(CAS_TURN, "inProgress")
                    }),
                ),
                user_started(),
                user_completed(),
                TurnStartAction::dynamic_tool_call(
                    713,
                    json!({
                        "arguments": { "question": "where?" },
                        "callId": "phase13-tool-call",
                        "namespace": "beryl",
                        "threadId": CAS_THREAD,
                        "tool": "resolve_discussion",
                        "turnId": CAS_TURN
                    }),
                    expected_response,
                ),
                notify(
                    "turn/completed",
                    json!({
                        "threadId": CAS_THREAD,
                        "turn": turn(CAS_TURN, "completed")
                    }),
                ),
            ],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(61));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let mut tools = RecordingTools::default();

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut tools,
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("expected terminal ordinary execution, got {outcome:?}")
    };
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(status.incomplete_reason(), None);
    drop(projection);
    assert_eq!(tools.calls.len(), 1);
    let call = &tools.calls[0];
    assert_eq!(call.thread, fixture.thread);
    assert_eq!(call.turn, submitted.turn);
    assert_eq!(call.cas_thread, CAS_THREAD);
    assert_eq!(call.cas_turn, CAS_TURN);
    assert_eq!(call.call, "phase13-tool-call");
    assert_eq!(call.namespace.as_deref(), Some("beryl"));
    assert_eq!(call.tool, "resolve_discussion");
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn terminal_capture_remains_count_independent_across_item_audit_pages() {
    const ASSISTANT_COUNT: u64 = 70;

    let mut fixture = Fixture::new(63);
    let submitted = fixture.submit_text(INPUT);
    let mut after_reply = vec![turn_started()];
    after_reply.push(user_started());
    after_reply.push(user_completed());
    for index in 0..ASSISTANT_COUNT {
        let item = agent_item(&format!("phase13-many-item-{index}"), "");
        after_reply.push(notify(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": item.clone()
            }),
        ));
        after_reply.push(notify(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": item
            }),
        ));
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
    let mut session = server.admit(execution_binding().runtime_id(), process(63));
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
    assert_eq!(state.record().item_count(), ASSISTANT_COUNT + 1);
    assert_eq!(state.record().finalized_item_count(), ASSISTANT_COUNT + 1);
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}
