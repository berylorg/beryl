use super::*;

#[test]
fn context_reopen_checks_finalization_kind_association_and_text() {
    assert_context_rejection(
        "context-special-intent-live-gate",
        "special draft has live or non-idle input authority",
        batch([FixtureRecord::InputGate(
            InputGateRecord::new(
                id(36),
                InputGateRevision::new(1).unwrap(),
                InputGateState::Idle,
                0,
                None,
                None,
                0,
                1,
                0,
            )
            .unwrap(),
        )]),
    );
    assert_context_rejection(
        "context-source-not-terminal",
        "context source turn is not finalized terminal history",
        unknown_terminal_source_mutation(),
    );
    assert_context_rejection(
        "context-projection-not-current",
        "context source projection is outside its current item set",
        batch([FixtureRecord::ContextEnvelope(
            context_record_with_projection(
                DiscussionContextOwnerId::Draft(draft_id(37)),
                source_resource_projection(),
                "assistant",
            ),
        )]),
    );
    assert_context_rejection(
        "context-source-not-assistant",
        "context source item is not an assistant message",
        context_source_user_item_mutation(),
    );
    assert_context_rejection(
        "context-source-revision",
        "context source records disagree",
        batch([FixtureRecord::ContextEnvelope(
            context_record_with_projection_revision(
                DiscussionContextOwnerId::Draft(draft_id(37)),
                source_projection(),
                ProjectionRevision::new(2).unwrap(),
                "assistant",
            ),
        )]),
    );
    assert_context_rejection(
        "context-source-text",
        "context source range and exact text disagree",
        batch([FixtureRecord::ContextEnvelope(context_record_with_text(
            DiscussionContextOwnerId::Draft(draft_id(37)),
            "assistAnt",
        ))]),
    );
}
