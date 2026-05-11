use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadMemberBinding, ConversationThreadTitleSource,
    ConversationThreadTokenUsageSnapshot, ConversationTokenUsageBreakdown, ConversationTurnId,
    PrimaryWorkspaceMember, RegisteredConversationThread, ThreadAutomaticTitleGenerationState,
    WorkspaceConversationState, WorkspaceConversationStateError,
};
use beryl_model::workspace::{RuntimeMode, WorkspaceId, WorkspaceMemberAvailability};

#[test]
fn primary_member_falls_back_to_implicit_home_without_explicit_members() {
    let mut state = WorkspaceConversationState::default();

    state.select_runtime(RuntimeMode::HostWindows).unwrap();

    match state.primary_member().unwrap() {
        PrimaryWorkspaceMember::ImplicitHome(RuntimeMode::HostWindows) => {}
        other => panic!("expected implicit host home member, got {other:?}"),
    }
}

#[test]
fn designating_first_execution_target_attaches_primary_member() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();

    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();

    assert_eq!(
        state.selected_runtime(),
        Some(execution_target.runtime_mode())
    );
    assert_eq!(state.explicit_members().len(), 1);
    assert_eq!(
        state.primary_explicit_member().unwrap().canonical_path(),
        execution_target.canonical_path()
    );
}

#[test]
fn detaching_current_primary_falls_back_to_first_remaining_member() {
    let mut state = WorkspaceConversationState::default();
    let first = state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\one"))
        .unwrap();
    assert!(first);
    state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\two"))
        .unwrap();
    let first_member_id = state.explicit_members()[0].id().clone();
    let second_member_id = state.explicit_members()[1].id().clone();

    state
        .set_primary_explicit_member(&second_member_id)
        .unwrap();
    state.detach_explicit_member(&second_member_id).unwrap();

    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &first_member_id
    );
}

#[test]
fn attaching_secondary_execution_target_preserves_existing_primary_member() {
    let mut state = WorkspaceConversationState::default();
    let first_target = WorkspaceId::host_windows(r"C:\work\one");
    let second_target = WorkspaceId::host_windows(r"C:\work\two");

    state
        .designate_primary_execution_target(&first_target)
        .unwrap();
    let first_member_id = state.primary_explicit_member().unwrap().id().clone();

    state.attach_execution_target(&second_target).unwrap();

    assert_eq!(state.explicit_members().len(), 2);
    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &first_member_id
    );
}

#[test]
fn explicit_members_retain_runtime_identity() {
    let mut state = WorkspaceConversationState::default();
    let host_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let wsl_target = WorkspaceId::wsl_linux("Debian", r"\work\beryl");

    state
        .designate_primary_execution_target(&host_target)
        .unwrap();
    state.attach_execution_target(&wsl_target).unwrap();

    assert_eq!(state.explicit_members().len(), 2);
    assert_eq!(
        state.explicit_members()[0].runtime_mode(),
        host_target.runtime_mode()
    );
    assert_eq!(
        state.explicit_members()[1].runtime_mode(),
        wsl_target.runtime_mode()
    );
    assert_eq!(state.default_runtime(), Some(host_target.runtime_mode()));
}

#[test]
fn same_canonical_path_can_be_attached_in_distinct_runtimes() {
    let mut state = WorkspaceConversationState::default();
    let host_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let wsl_target = WorkspaceId::wsl_linux("Debian", r"C:\work\beryl");

    state.attach_execution_target(&host_target).unwrap();
    state.attach_execution_target(&wsl_target).unwrap();

    assert_eq!(state.explicit_members().len(), 2);
}

#[test]
fn unavailable_primary_promotes_to_next_available_member_durably() {
    let mut state = WorkspaceConversationState::default();
    let first_target = WorkspaceId::host_windows(r"C:\work\one");
    let second_target = WorkspaceId::host_windows(r"C:\work\two");

    state
        .designate_primary_execution_target(&first_target)
        .unwrap();
    state.attach_execution_target(&second_target).unwrap();
    let first_member_id = state.explicit_members()[0].id().clone();
    let second_member_id = state.explicit_members()[1].id().clone();

    assert!(
        state
            .mark_explicit_member_path_not_found(&first_member_id)
            .unwrap()
    );

    assert_eq!(
        state.durable_primary_explicit_member_id(),
        Some(&second_member_id)
    );
    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &second_member_id
    );
    assert_eq!(
        state
            .unavailable_explicit_members()
            .next()
            .unwrap()
            .availability(),
        WorkspaceMemberAvailability::PathNotFound
    );
}

#[test]
fn all_unavailable_explicit_members_fall_back_to_implicit_home_durably() {
    let mut state = WorkspaceConversationState::default();
    let target = WorkspaceId::host_windows(r"C:\work\one");

    state.designate_primary_execution_target(&target).unwrap();
    let member_id = state.explicit_members()[0].id().clone();

    state
        .mark_explicit_member_path_not_found(&member_id)
        .unwrap();

    assert_eq!(state.durable_primary_explicit_member_id(), None);
    assert_eq!(state.primary_explicit_member(), None);
    match state.primary_member().unwrap() {
        PrimaryWorkspaceMember::ImplicitHome(RuntimeMode::HostWindows) => {}
        other => panic!("expected implicit host home member, got {other:?}"),
    }
}

#[test]
fn returning_member_does_not_restore_primary_automatically() {
    let mut state = WorkspaceConversationState::default();
    let first_target = WorkspaceId::host_windows(r"C:\work\one");
    let second_target = WorkspaceId::host_windows(r"C:\work\two");

    state
        .designate_primary_execution_target(&first_target)
        .unwrap();
    state.attach_execution_target(&second_target).unwrap();
    let first_member_id = state.explicit_members()[0].id().clone();
    let second_member_id = state.explicit_members()[1].id().clone();

    state
        .mark_explicit_member_path_not_found(&first_member_id)
        .unwrap();
    state
        .mark_explicit_member_available(&first_member_id)
        .unwrap();

    assert_eq!(
        state.durable_primary_explicit_member_id(),
        Some(&second_member_id)
    );
    assert_eq!(
        state.primary_explicit_member().unwrap().id(),
        &second_member_id
    );
}

#[test]
fn overlapping_members_are_rejected() {
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\beryl"))
        .unwrap();

    let error = state
        .designate_primary_execution_target(&WorkspaceId::host_windows(r"C:\work\beryl\src"))
        .unwrap_err();

    assert!(matches!(
        error,
        WorkspaceConversationStateError::WorkspaceMemberOverlap { .. }
    ));
}

#[test]
fn default_runtime_change_keeps_explicit_members_runtime_bound() {
    let mut state = WorkspaceConversationState::default();
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();

    assert!(
        state
            .select_default_runtime(RuntimeMode::WslLinux {
                distro_name: "Debian".to_string(),
            })
            .unwrap()
    );

    assert_eq!(
        state.default_runtime(),
        Some(&RuntimeMode::WslLinux {
            distro_name: "Debian".to_string(),
        })
    );
    assert_eq!(
        state.explicit_members()[0].runtime_mode(),
        &RuntimeMode::HostWindows
    );
    assert_eq!(
        state.explicit_members()[0].canonical_path(),
        execution_target.canonical_path()
    );
}

#[test]
fn remember_thread_keeps_threads_sorted_by_recent_activity() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let older = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_old"),
        execution_target.clone(),
        "Old thread",
        None,
        1,
        2,
    );
    let newer = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_new"),
        execution_target,
        "New thread",
        None,
        3,
        4,
    );

    let mut state = WorkspaceConversationState::default();
    state.remember_thread(older);
    state.remember_thread(newer);

    let ordered: Vec<_> = state
        .threads()
        .iter()
        .map(|thread| thread.thread_id().as_str())
        .collect();
    assert_eq!(ordered, vec!["thread_new", "thread_old"]);
}

#[test]
fn remembered_thread_records_backend_name_snapshot_from_backend_metadata() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread = RegisteredConversationThread::new(
        ConversationThreadId::new("thread_named"),
        execution_target,
        "Named thread",
        Some("Release notes".to_string()),
        7,
        8,
    );

    assert_eq!(thread.backend_name(), Some("Release notes"));
    assert_eq!(thread.title(), Some("Release notes"));
    assert!(thread.gui_title().is_none());
}

#[test]
fn remembering_existing_thread_preserves_backend_name_snapshot_from_stale_summary() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_named");
    let mut state = WorkspaceConversationState::default();

    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Named preview",
        Some("Release notes".to_string()),
        7,
        8,
    ));

    assert!(state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Stale unnamed preview",
        None,
        7,
        9,
    )));

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), Some("Release notes"));
    assert_eq!(thread.title(), Some("Release notes"));
    assert_eq!(thread.preview(), "Stale unnamed preview");

    assert!(state.set_thread_backend_name(&thread_id, None).unwrap());
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), None);
    assert_eq!(thread.title(), None);
}

#[test]
fn remembering_existing_thread_ignores_suppressed_automatic_title_backend_name() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut state = WorkspaceConversationState::default();

    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target.clone(),
            "Branch preview",
            None,
            1,
            2,
        )
        .with_beryl_created()
        .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );
    state
        .mark_thread_automatic_title_generation_started(&thread_id)
        .unwrap();

    assert!(state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Refreshed branch preview",
        Some("Source title".to_string()),
        3,
        4,
    )));

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), None);
    assert_eq!(
        thread.ignored_backend_name_for_automatic_title(),
        Some("Source title")
    );
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::InFlight
    );
}

#[test]
fn remembering_existing_thread_accepts_distinct_backend_name_after_suppression() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut state = WorkspaceConversationState::default();

    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target.clone(),
            "Branch preview",
            None,
            1,
            2,
        )
        .with_beryl_created()
        .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );
    state
        .mark_thread_automatic_title_generation_started(&thread_id)
        .unwrap();

    assert!(state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Refreshed branch preview",
        Some("Generated branch title".to_string()),
        3,
        4,
    )));

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), Some("Generated branch title"));
    assert_eq!(thread.ignored_backend_name_for_automatic_title(), None);
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Applied
    );
}

#[test]
fn backend_name_update_ignores_suppressed_automatic_title_backend_name_unless_authoritative() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_branch");
    let mut state = WorkspaceConversationState::default();

    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target,
            "Branch preview",
            None,
            1,
            2,
        )
        .with_beryl_created()
        .with_ignored_backend_name_for_automatic_title(Some("Source title".to_string())),
    );
    state
        .mark_thread_automatic_title_generation_started(&thread_id)
        .unwrap();

    assert!(
        !state
            .set_thread_backend_name(&thread_id, Some("Source title".to_string()))
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), None);
    assert_eq!(
        thread.ignored_backend_name_for_automatic_title(),
        Some("Source title")
    );
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::InFlight
    );

    assert!(
        state
            .set_authoritative_thread_backend_name(&thread_id, Some("Source title".to_string()))
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), Some("Source title"));
    assert_eq!(thread.ignored_backend_name_for_automatic_title(), None);
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Applied
    );
}

#[test]
fn generated_thread_title_is_persisted_without_overwriting_existing_title() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));

    assert!(
        state
            .set_thread_generated_title_if_absent(
                &ConversationThreadId::new("thread_1"),
                "Generated title",
                9,
            )
            .unwrap()
    );
    assert!(
        !state
            .set_thread_generated_title_if_absent(
                &ConversationThreadId::new("thread_1"),
                "Second generated title",
                10,
            )
            .unwrap()
    );

    let thread = state
        .thread_registration(&ConversationThreadId::new("thread_1"))
        .unwrap();
    assert_eq!(thread.title(), Some("Generated title"));
    assert_eq!(
        thread.gui_title().unwrap().source(),
        ConversationThreadTitleSource::FirstCompletedTurn
    );
}

#[test]
fn generated_thread_title_is_not_set_when_backend_name_exists() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Preview",
        Some("Backend title".to_string()),
        1,
        2,
    ));

    assert!(
        !state
            .set_thread_generated_title_if_absent(&thread_id, "Generated title", 9)
            .unwrap()
    );

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.title(), Some("Backend title"));
    assert!(thread.gui_title().is_none());
}

#[test]
fn backend_thread_name_updates_override_generated_fallback_without_overwriting_it() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_generated_title_if_absent(&thread_id, "Generated title", 9)
        .unwrap();

    assert!(
        state
            .set_thread_backend_name(&thread_id, Some(" Backend title ".to_string()))
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), Some("Backend title"));
    assert_eq!(thread.title(), Some("Backend title"));
    assert_eq!(
        thread.gui_title().unwrap().source(),
        ConversationThreadTitleSource::FirstCompletedTurn
    );

    assert!(state.set_thread_backend_name(&thread_id, None).unwrap());
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), None);
    assert_eq!(thread.title(), Some("Generated title"));
}

#[test]
fn backend_thread_name_updates_do_not_override_manual_title() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Preview",
        Some("Initial backend".to_string()),
        1,
        2,
    ));
    state
        .set_thread_manual_title(&thread_id, "Manual title", 9)
        .unwrap();

    assert!(
        state
            .set_thread_backend_name(&thread_id, Some("Updated backend".to_string()))
            .unwrap()
    );

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.backend_name(), Some("Updated backend"));
    assert_eq!(thread.title(), Some("Manual title"));
    assert_eq!(
        thread.gui_title().unwrap().source(),
        ConversationThreadTitleSource::Manual
    );
}

#[test]
fn automatic_thread_title_generation_lifecycle_distinguishes_retryable_and_terminal_states() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_title");
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(
        RegisteredConversationThread::new(
            thread_id.clone(),
            execution_target,
            "Preview",
            None,
            1,
            2,
        )
        .with_beryl_created(),
    );

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::NotStarted
    );
    assert!(state.thread_automatic_title_generation_eligible(&thread_id));

    assert!(
        state
            .mark_thread_automatic_title_generation_started(&thread_id)
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::InFlight
    );
    assert!(!state.thread_automatic_title_generation_eligible(&thread_id));
    assert!(
        !state
            .mark_thread_automatic_title_generation_started(&thread_id)
            .unwrap()
    );

    assert!(
        state
            .mark_thread_automatic_title_generation_abandoned(&thread_id)
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Abandoned
    );
    assert!(state.thread_automatic_title_generation_eligible(&thread_id));

    assert!(
        state
            .mark_thread_automatic_title_generation_started(&thread_id)
            .unwrap()
    );
    assert!(
        state
            .set_thread_backend_name(&thread_id, Some(" Backend title ".to_string()))
            .unwrap()
    );
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Applied
    );
    assert!(!state.thread_automatic_title_generation_eligible(&thread_id));
    assert!(
        !state
            .mark_thread_automatic_title_generation_abandoned(&thread_id)
            .unwrap()
    );

    assert!(state.set_thread_backend_name(&thread_id, None).unwrap());
    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Applied
    );
    assert!(!state.thread_automatic_title_generation_eligible(&thread_id));
}

#[test]
fn thread_token_usage_snapshot_is_recorded_and_replaced_by_thread_id() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let first = token_usage_snapshot("turn_1", 140, Some(200_000), 10);
    let replacement = token_usage_snapshot("turn_2", 180, Some(200_000), 20);
    let mut state = WorkspaceConversationState::default();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));

    assert!(
        state
            .record_thread_token_usage_snapshot(&thread_id, first.clone())
            .unwrap()
    );
    assert!(
        !state
            .record_thread_token_usage_snapshot(&thread_id, first)
            .unwrap()
    );
    assert!(
        state
            .record_thread_token_usage_snapshot(&thread_id, replacement.clone())
            .unwrap()
    );

    assert_eq!(
        state.thread_token_usage_snapshot(&thread_id),
        Some(&replacement)
    );
    assert!(matches!(
        state.record_thread_token_usage_snapshot(
            &ConversationThreadId::new("missing_thread"),
            replacement,
        ),
        Err(WorkspaceConversationStateError::MissingThread { .. })
    ));
}

#[test]
fn remembering_existing_thread_preserves_gui_title_binding_and_rebind_requirement() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let snapshot = token_usage_snapshot("turn_1", 150, Some(200_000), 4);
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let member_id = state.primary_explicit_member().unwrap().id().clone();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Initial preview",
        None,
        1,
        2,
    ));
    state
        .set_thread_manual_title(&thread_id, "Manual title", 3)
        .unwrap();
    state
        .mark_thread_rebind_required(&thread_id, "Explicit rebind required")
        .unwrap();
    state
        .record_thread_token_usage_snapshot(&thread_id, snapshot.clone())
        .unwrap();

    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target,
        "Updated preview",
        Some("Backend title".to_string()),
        1,
        9,
    ));

    let thread = state.thread_registration(&thread_id).unwrap();
    assert_eq!(thread.preview(), "Updated preview");
    assert_eq!(thread.backend_name(), Some("Backend title"));
    assert_eq!(thread.title(), Some("Manual title"));
    assert_eq!(
        thread.gui_title().unwrap().source(),
        ConversationThreadTitleSource::Manual
    );
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::Explicit {
            member_id: bound_member_id,
            ..
        }) if bound_member_id == &member_id
    ));
    assert_eq!(
        thread.rebind_required().unwrap().detail(),
        "Explicit rebind required"
    );
    assert_eq!(thread.token_usage_snapshot(), Some(&snapshot));
}

#[test]
fn legacy_thread_records_without_token_usage_snapshot_deserialize() {
    let legacy_json = r#"{
        "threads": [
            {
                "thread_id": "thread_1",
                "execution_target": {
                    "runtime_mode": "HostWindows",
                    "canonical_path": "C:\\work\\beryl"
                },
                "preview": "Legacy preview",
                "created_at_millis": 1,
                "updated_at_millis": 2
            }
        ],
        "active_thread": "thread_1"
    }"#;

    let state: WorkspaceConversationState = serde_json::from_str(legacy_json).unwrap();
    let thread = state
        .thread_registration(&ConversationThreadId::new("thread_1"))
        .unwrap();

    assert!(thread.token_usage_snapshot().is_none());
}

#[test]
fn legacy_attempted_automatic_title_generation_without_title_deserializes_as_retryable() {
    let legacy_json = r#"{
        "threads": [
            {
                "thread_id": "thread_1",
                "execution_target": {
                    "runtime_mode": "HostWindows",
                    "canonical_path": "C:\\work\\beryl"
                },
                "preview": "Legacy preview",
                "beryl_created": true,
                "automatic_title_generation_attempted": true,
                "created_at_millis": 1,
                "updated_at_millis": 2
            }
        ],
        "active_thread": "thread_1"
    }"#;
    let thread_id = ConversationThreadId::new("thread_1");

    let state: WorkspaceConversationState = serde_json::from_str(legacy_json).unwrap();
    let thread = state.thread_registration(&thread_id).unwrap();

    assert_eq!(
        thread.automatic_title_generation_state(),
        ThreadAutomaticTitleGenerationState::Abandoned
    );
    assert!(thread.automatic_title_generation_attempted());
    assert!(state.thread_automatic_title_generation_eligible(&thread_id));
}

#[test]
fn runtime_change_without_explicit_members_marks_implicit_threads_rebind_required() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let thread_id = ConversationThreadId::new("thread_home");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        home_target,
        "Home preview",
        None,
        1,
        2,
    ));

    state
        .select_runtime(RuntimeMode::WslLinux {
            distro_name: "Debian".to_string(),
        })
        .unwrap();

    let thread = state.thread_registration(&thread_id).unwrap();
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::ImplicitHome { .. })
    ));
    assert!(thread.requires_rebind());
    assert!(
        thread
            .rebind_required()
            .unwrap()
            .detail()
            .contains("runtime environment")
    );
}

#[test]
fn remembering_thread_binds_it_to_matching_explicit_member() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let member_id = state.primary_explicit_member().unwrap().id().clone();

    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));

    let thread = state
        .thread_registration(&ConversationThreadId::new("thread_1"))
        .unwrap();
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::Explicit {
            member_id: bound_member_id,
            ..
        }) if bound_member_id == &member_id
    ));
}

#[test]
fn detaching_bound_member_marks_thread_rebind_required() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let member_id = state.primary_explicit_member().unwrap().id().clone();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_1"),
        execution_target,
        "Preview",
        None,
        1,
        2,
    ));

    state.detach_explicit_member(&member_id).unwrap();

    let thread = state
        .thread_registration(&ConversationThreadId::new("thread_1"))
        .unwrap();
    assert!(thread.requires_rebind());
    assert!(
        thread
            .rebind_required()
            .unwrap()
            .detail()
            .contains("detached")
    );
}

#[test]
fn returning_unavailable_member_restores_matching_thread_binding() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let member_id = state.primary_explicit_member().unwrap().id().clone();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Preview",
        None,
        1,
        2,
    ));

    state
        .mark_explicit_member_path_not_found(&member_id)
        .unwrap();
    assert!(
        state
            .thread_registration(&thread_id)
            .unwrap()
            .requires_rebind()
    );

    assert!(state.mark_explicit_member_available(&member_id).unwrap());

    let thread = state.thread_registration(&thread_id).unwrap();
    assert!(!thread.requires_rebind());
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::Explicit {
            member_id: bound_member_id,
            execution_target: bound_target,
        }) if bound_member_id == &member_id && bound_target == &execution_target
    ));
}

#[test]
fn reattaching_same_target_after_detach_restores_matching_thread_binding() {
    let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_1");
    let mut state = WorkspaceConversationState::default();
    state
        .designate_primary_execution_target(&execution_target)
        .unwrap();
    let original_member_id = state.primary_explicit_member().unwrap().id().clone();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        execution_target.clone(),
        "Preview",
        None,
        1,
        2,
    ));

    state.detach_explicit_member(&original_member_id).unwrap();
    assert!(
        state
            .thread_registration(&thread_id)
            .unwrap()
            .requires_rebind()
    );

    state.attach_execution_target(&execution_target).unwrap();

    let restored_member_id = state.primary_explicit_member().unwrap().id().clone();
    assert_ne!(restored_member_id, original_member_id);
    let thread = state.thread_registration(&thread_id).unwrap();
    assert!(!thread.requires_rebind());
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::Explicit {
            member_id: bound_member_id,
            execution_target: bound_target,
        }) if bound_member_id == &restored_member_id && bound_target == &execution_target
    ));
}

#[test]
fn workspace_scope_requires_exact_implicit_home_target() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let missing_member_target = WorkspaceId::host_windows(r"C:\work\missing");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();

    assert!(state.execution_target_in_workspace_scope(&home_target, Some(&home_target)));
    assert!(!state.execution_target_in_workspace_scope(&missing_member_target, Some(&home_target)));
}

#[test]
fn attaching_first_explicit_member_marks_implicit_home_threads_rebind_required() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let explicit_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        ConversationThreadId::new("thread_home"),
        home_target,
        "Home preview",
        None,
        1,
        2,
    ));

    state.attach_execution_target(&explicit_target).unwrap();

    let thread = state
        .thread_registration(&ConversationThreadId::new("thread_home"))
        .unwrap();
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::ImplicitHome { .. })
    ));
    assert!(thread.requires_rebind());
}

#[test]
fn implicit_home_threads_restore_when_home_fallback_returns() {
    let home_target = WorkspaceId::host_windows(r"C:\Users\operator");
    let explicit_target = WorkspaceId::host_windows(r"C:\work\beryl");
    let thread_id = ConversationThreadId::new("thread_home");
    let mut state = WorkspaceConversationState::default();
    state.select_runtime(RuntimeMode::HostWindows).unwrap();
    state.remember_thread(RegisteredConversationThread::new(
        thread_id.clone(),
        home_target.clone(),
        "Home preview",
        None,
        1,
        2,
    ));
    state.attach_execution_target(&explicit_target).unwrap();
    let explicit_member_id = state.primary_explicit_member().unwrap().id().clone();
    assert!(
        state
            .thread_registration(&thread_id)
            .unwrap()
            .requires_rebind()
    );

    state.detach_explicit_member(&explicit_member_id).unwrap();
    assert!(state.restore_implicit_home_threads_for_execution_target(&home_target));

    let thread = state.thread_registration(&thread_id).unwrap();
    assert!(!thread.requires_rebind());
    assert!(matches!(
        thread.member_binding(),
        Some(ConversationThreadMemberBinding::ImplicitHome {
            execution_target
        }) if execution_target == &home_target
    ));
}

fn token_usage_snapshot(
    turn_id: &str,
    input_tokens: i64,
    model_context_window: Option<i64>,
    observed_at_millis: u64,
) -> ConversationThreadTokenUsageSnapshot {
    ConversationThreadTokenUsageSnapshot::new(
        ConversationTurnId::new(turn_id),
        ConversationTokenUsageBreakdown::new(2, input_tokens, 5, 7, input_tokens + 14),
        ConversationTokenUsageBreakdown::new(3, input_tokens + 20, 11, 13, input_tokens + 47),
        model_context_window,
        observed_at_millis,
    )
}
