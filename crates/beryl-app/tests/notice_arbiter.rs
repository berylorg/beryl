use beryl_app::main_window::*;
use beryl_model::WindowId;

fn window(value: u8) -> WindowId {
    WindowId::from_bytes([value; 16])
}

fn content(text: &str, dismissal: NoticeDismissal) -> NoticeContent {
    NoticeContent::new(NoticeVariant::Warning, dismissal, text, text)
}

fn record(kind: NoticeKind, text: &str) -> NoticeRecord {
    NoticeRecord {
        window_id: window(1),
        condition: NoticeConditionId::new(),
        revision: 1,
        kind,
        content: content(
            text,
            if matches!(
                kind,
                NoticeKind::HomeFailure | NoticeKind::RuntimeUnavailable
            ) {
                NoticeDismissal::Persistent
            } else {
                NoticeDismissal::Dismissible
            },
        ),
    }
}

fn admitted(arbiter: &mut MainWindowNoticeArbiter, record: NoticeRecord) -> NoticeRecordToken {
    match arbiter.admit(record) {
        NoticeAdmission::Admitted(token) => token,
        result => panic!("unexpected admission: {result:?}"),
    }
}

fn visible(arbiter: &MainWindowNoticeArbiter) -> NoticeVisibleToken {
    arbiter.active().unwrap().token
}

fn assert_title(arbiter: &MainWindowNoticeArbiter, expected: &str) {
    assert_eq!(arbiter.active().unwrap().content.title().as_str(), expected);
}

fn fill_information(arbiter: &mut MainWindowNoticeArbiter) -> Vec<NoticeRecordToken> {
    (0..NOTICE_GENERAL_CAPACITY)
        .map(|index| admitted(arbiter, record(NoticeKind::Information, &index.to_string())))
        .collect()
}

#[test]
fn priority_and_fifo_resume_the_oldest_preempted_record() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let ordered = [
        NoticeKind::HomeFailure,
        NoticeKind::ExactStopFeedback,
        NoticeKind::Lifecycle,
        NoticeKind::RuntimeUnavailable,
        NoticeKind::Error,
        NoticeKind::Recovery,
        NoticeKind::Warning,
        NoticeKind::Information,
    ];
    for kind in ordered.into_iter().rev() {
        admitted(&mut arbiter, record(kind, &format!("{kind:?}")));
        assert_eq!(arbiter.active().unwrap().kind, kind);
    }
    for kind in ordered {
        assert_eq!(arbiter.active().unwrap().kind, kind);
        arbiter.remove(visible(&arbiter).record()).unwrap();
    }
    assert!(arbiter.active().is_none());

    admitted(&mut arbiter, record(NoticeKind::Warning, "oldest"));
    admitted(&mut arbiter, record(NoticeKind::Warning, "second"));
    let high = admitted(&mut arbiter, record(NoticeKind::Error, "higher"));
    admitted(&mut arbiter, record(NoticeKind::Warning, "third"));
    assert_title(&arbiter, "higher");
    arbiter.remove(&high).unwrap();
    for title in ["oldest", "second", "third"] {
        assert_title(&arbiter, title);
        arbiter.dismiss(&visible(&arbiter)).unwrap();
    }
}

#[test]
fn all_protected_conditions_fit_after_general_saturation_and_survive_lifecycle_pressure() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    fill_information(&mut arbiter);
    let home = admitted(&mut arbiter, record(NoticeKind::HomeFailure, "home"));
    let runtime = admitted(
        &mut arbiter,
        record(NoticeKind::RuntimeUnavailable, "runtime"),
    );
    let stop = admitted(&mut arbiter, record(NoticeKind::ExactStopFeedback, "stop"));
    assert_eq!(
        arbiter.diagnostics().retained_records,
        NOTICE_RECORD_CAPACITY
    );
    assert_eq!(
        arbiter.diagnostics().pending_records,
        NOTICE_RECORD_CAPACITY - 1
    );
    for _ in 0..NOTICE_GENERAL_CAPACITY {
        admitted(&mut arbiter, record(NoticeKind::Lifecycle, "lifecycle"));
    }
    assert_eq!(
        arbiter.admit(record(NoticeKind::Lifecycle, "omitted")),
        NoticeAdmission::Omitted
    );
    assert_eq!(
        arbiter.diagnostics().retained_records,
        NOTICE_RECORD_CAPACITY
    );
    arbiter.remove(&home).unwrap();
    assert_title(&arbiter, "stop");
    arbiter.remove(&stop).unwrap();
    assert_eq!(arbiter.active().unwrap().kind, NoticeKind::Lifecycle);
    arbiter.remove(&runtime).unwrap();
    assert_eq!(arbiter.diagnostics().omitted, 1);
}

#[test]
fn replacement_chooses_newest_pending_at_the_lowest_lower_priority() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let tokens = fill_information(&mut arbiter);
    let warning = admitted(&mut arbiter, record(NoticeKind::Warning, "warning"));
    assert_eq!(
        arbiter.remove(tokens.last().unwrap()),
        Err(NoticeRejection::StaleRecord)
    );
    assert_eq!(arbiter.diagnostics().replaced, 1);
    arbiter.remove(&warning).unwrap();
    for index in 0..NOTICE_GENERAL_CAPACITY - 1 {
        assert_title(&arbiter, &index.to_string());
        arbiter.dismiss(&visible(&arbiter)).unwrap();
    }
    assert!(arbiter.active().is_none());
}

#[test]
fn lowest_priority_wins_eviction_over_newer_higher_priority_records() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    admitted(&mut arbiter, record(NoticeKind::Error, "active"));
    let info = admitted(&mut arbiter, record(NoticeKind::Information, "older info"));
    for _ in 2..NOTICE_GENERAL_CAPACITY {
        admitted(&mut arbiter, record(NoticeKind::Warning, "newer warning"));
    }
    admitted(&mut arbiter, record(NoticeKind::Recovery, "recovery"));
    assert_eq!(arbiter.remove(&info), Err(NoticeRejection::StaleRecord));
    assert_title(&arbiter, "active");
}

#[test]
fn equal_and_lower_arrivals_are_omitted_without_merging_conditions() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    for _ in 0..NOTICE_GENERAL_CAPACITY {
        admitted(&mut arbiter, record(NoticeKind::Error, "same content"));
    }
    for kind in [
        NoticeKind::Error,
        NoticeKind::Warning,
        NoticeKind::Information,
    ] {
        assert_eq!(
            arbiter.admit(record(kind, "same content")),
            NoticeAdmission::Omitted
        );
    }
    assert_eq!(arbiter.diagnostics().omitted, 3);
    assert_eq!(
        arbiter.diagnostics().retained_records,
        NOTICE_GENERAL_CAPACITY
    );
    assert_eq!(arbiter.active().unwrap().report_count, 1);
}

#[test]
fn newer_same_condition_updates_in_place_and_stale_revisions_cannot_overwrite_it() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let mut original = record(NoticeKind::Warning, "initial");
    let old = admitted(&mut arbiter, original.clone());
    let old_visible = visible(&arbiter);
    admitted(&mut arbiter, record(NoticeKind::Warning, "waiting"));
    original.revision = 2;
    original.content = content("updated", NoticeDismissal::Dismissible)
        .with_commands(&[NoticeCommand::enabled(
            NoticeCommandId::new(1),
            "owner command",
        )])
        .unwrap();
    let NoticeAdmission::Updated(new) = arbiter.admit(original.clone()) else {
        panic!("not updated")
    };
    assert_title(&arbiter, "updated");
    assert_eq!(arbiter.active().unwrap().report_count, 2);
    assert_eq!(arbiter.active().unwrap().content.commands().count(), 1);
    assert_eq!(arbiter.diagnostics().retained_records, 2);
    assert_eq!(
        arbiter.admit(original.clone()),
        NoticeAdmission::Rejected(NoticeRejection::StaleRevision)
    );
    original.revision = 0;
    assert_eq!(
        arbiter.admit(original),
        NoticeAdmission::Rejected(NoticeRejection::StaleRevision)
    );
    assert_eq!(arbiter.remove(&old), Err(NoticeRejection::StaleRecord));
    assert_eq!(
        arbiter.dismiss(&old_visible),
        Err(NoticeRejection::StaleRecord)
    );
    assert_eq!(
        arbiter.update(&old, 3, content("stale", NoticeDismissal::Dismissible)),
        Err(NoticeRejection::StaleRecord)
    );
    arbiter.remove(&new).unwrap();
    assert_title(&arbiter, "waiting");
}

#[test]
fn pending_update_preserves_fifo_and_preemption_refreshes_visible_identity() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let first = admitted(&mut arbiter, record(NoticeKind::Warning, "first"));
    let stale_visible = visible(&arbiter);
    admitted(&mut arbiter, record(NoticeKind::Warning, "second"));
    let high = admitted(&mut arbiter, record(NoticeKind::Error, "high"));
    assert_eq!(
        arbiter.dismiss(&stale_visible),
        Err(NoticeRejection::StaleVisibility)
    );
    arbiter.remove(&high).unwrap();
    assert_eq!(
        arbiter.dismiss(&stale_visible),
        Err(NoticeRejection::StaleVisibility)
    );
    let high = admitted(&mut arbiter, record(NoticeKind::Error, "high again"));
    let revised = arbiter
        .update(
            &first,
            2,
            content("revised first", NoticeDismissal::Dismissible),
        )
        .unwrap();
    arbiter.remove(&high).unwrap();
    assert_title(&arbiter, "revised first");
    assert_eq!(visible(&arbiter).record(), &revised);
    arbiter.dismiss(&visible(&arbiter)).unwrap();
    assert_title(&arbiter, "second");
}

#[test]
fn persistent_conditions_require_owner_removal_and_exact_stop_can_converge() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let mut stop = record(NoticeKind::ExactStopFeedback, "awaiting");
    stop.content.dismissal = NoticeDismissal::Persistent;
    let token = admitted(&mut arbiter, stop);
    assert_eq!(
        arbiter.dismiss(&visible(&arbiter)),
        Err(NoticeRejection::Persistent)
    );
    let completed = NoticeContent::new(
        NoticeVariant::Info,
        NoticeDismissal::Dismissible,
        "completed",
        "",
    );
    arbiter.update(&token, 2, completed).unwrap();
    assert_eq!(
        arbiter.active().unwrap().kind,
        NoticeKind::ExactStopFeedback
    );
    assert_eq!(
        arbiter.active().unwrap().content.variant,
        NoticeVariant::Info
    );
    arbiter.dismiss(&visible(&arbiter)).unwrap();
    let mut ordinary = record(NoticeKind::Error, "unresolved");
    ordinary.content.dismissal = NoticeDismissal::Persistent;
    let persistent = admitted(&mut arbiter, ordinary);
    assert_eq!(
        arbiter.dismiss(&visible(&arbiter)),
        Err(NoticeRejection::Persistent)
    );
    arbiter.remove(&persistent).unwrap();
}

#[test]
fn persistent_home_and_runtime_cannot_become_dismissible() {
    for kind in [NoticeKind::HomeFailure, NoticeKind::RuntimeUnavailable] {
        let mut arbiter = MainWindowNoticeArbiter::new(window(1));
        let token = admitted(&mut arbiter, record(kind, "persistent"));
        assert_eq!(
            arbiter.update(&token, 2, content("invalid", NoticeDismissal::Dismissible)),
            Err(NoticeRejection::Persistent)
        );
        assert_title(&arbiter, "persistent");
        assert_eq!(
            arbiter.dismiss(&visible(&arbiter)),
            Err(NoticeRejection::Persistent)
        );
    }
}

#[test]
fn protected_successors_require_the_exact_old_condition_and_do_not_inherit_stale_actions() {
    for kind in [
        NoticeKind::HomeFailure,
        NoticeKind::RuntimeUnavailable,
        NoticeKind::ExactStopFeedback,
    ] {
        let mut arbiter = MainWindowNoticeArbiter::new(window(1));
        fill_information(&mut arbiter);
        let old = admitted(&mut arbiter, record(kind, "old condition"));
        let stale_visible = visible(&arbiter);
        let replacement = record(kind, "successor");
        assert_eq!(
            arbiter.admit(replacement.clone()),
            NoticeAdmission::Rejected(NoticeRejection::ProtectedConditionOccupied)
        );
        let new = arbiter.replace_protected(&old, replacement).unwrap();
        assert_title(&arbiter, "successor");
        assert_eq!(arbiter.remove(&old), Err(NoticeRejection::StaleRecord));
        assert_eq!(
            arbiter.dismiss(&stale_visible),
            Err(NoticeRejection::StaleRecord)
        );
        assert_eq!(
            arbiter.replace_protected(&old, record(kind, "late replacement")),
            Err(NoticeRejection::StaleRecord)
        );
        assert_eq!(
            arbiter.diagnostics().retained_records,
            NOTICE_GENERAL_CAPACITY + 1
        );
        arbiter.remove(&new).unwrap();
    }
}

#[test]
fn invalid_replacement_and_kind_changes_preserve_existing_records() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let source = record(NoticeKind::RuntimeUnavailable, "runtime");
    let token = admitted(&mut arbiter, source.clone());
    assert_eq!(
        arbiter.replace_protected(&token, record(NoticeKind::HomeFailure, "wrong class")),
        Err(NoticeRejection::InvalidProtectedReplacement)
    );
    assert_eq!(
        arbiter.replace_protected(&token, source.clone()),
        Err(NoticeRejection::InvalidProtectedReplacement)
    );
    let mut changed = source;
    changed.kind = NoticeKind::HomeFailure;
    changed.revision = 2;
    assert_eq!(
        arbiter.admit(changed),
        NoticeAdmission::Rejected(NoticeRejection::ConditionKindMismatch)
    );
    assert_title(&arbiter, "runtime");
    assert_eq!(arbiter.diagnostics().retained_records, 1);
}

#[test]
fn exact_window_and_arbiter_incarnation_isolate_identical_conditions() {
    let mut first = MainWindowNoticeArbiter::new(window(1));
    let mut second = MainWindowNoticeArbiter::new(window(2));
    let source = record(NoticeKind::Warning, "shared condition");
    let token = admitted(&mut first, source.clone());
    let first_visible = visible(&first);
    assert_eq!(
        second.admit(source.clone()),
        NoticeAdmission::Rejected(NoticeRejection::WrongWindow)
    );
    let mut other = source.clone();
    other.window_id = window(2);
    admitted(&mut second, other);
    assert_eq!(second.remove(&token), Err(NoticeRejection::WrongWindow));
    assert_eq!(
        second.dismiss(&first_visible),
        Err(NoticeRejection::WrongWindow)
    );
    let mut reincarnated = MainWindowNoticeArbiter::new(window(1));
    admitted(&mut reincarnated, source);
    assert_eq!(
        reincarnated.remove(&token),
        Err(NoticeRejection::StaleRecord)
    );
    assert_eq!(
        reincarnated.dismiss(&first_visible),
        Err(NoticeRejection::StaleRecord)
    );
    assert_title(&second, "shared condition");
    assert_title(&reincarnated, "shared condition");
}

#[test]
fn readmission_does_not_reuse_old_tokens_or_require_retained_tombstones() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let source = record(NoticeKind::Warning, "same eligible identity");
    let old = admitted(&mut arbiter, source.clone());
    arbiter.remove(&old).unwrap();
    assert_eq!(
        arbiter.update(&old, 2, source.content.clone()),
        Err(NoticeRejection::StaleRecord)
    );
    let new = admitted(&mut arbiter, source);
    assert_ne!(old, new);
    assert_eq!(arbiter.remove(&old), Err(NoticeRejection::StaleRecord));
    arbiter.remove(&new).unwrap();
    assert_eq!(arbiter.diagnostics().retained_records, 0);
}

#[test]
fn content_and_commands_are_bounded_utf8_and_debug_output_is_content_free() {
    let large = "秘密🔒".repeat(10_000);
    let commands = (0..NOTICE_COMMAND_CAPACITY)
        .map(|index| NoticeCommand::disabled(NoticeCommandId::new(index as u64), &large, &large))
        .collect::<Vec<_>>();
    let bounded = content(&large, NoticeDismissal::Dismissible)
        .with_commands(&commands)
        .unwrap();
    for (text, limit) in [
        (bounded.title(), NOTICE_TITLE_BYTES),
        (bounded.detail(), NOTICE_DETAIL_BYTES),
    ] {
        assert!(text.as_str().len() <= limit);
        assert!(text.is_truncated());
        assert!(text.as_str().ends_with('…'));
    }
    for command in bounded.commands() {
        assert!(command.label().as_str().len() <= NOTICE_COMMAND_LABEL_BYTES);
        assert!(command.disabled_reason().unwrap().as_str().len() <= NOTICE_COMMAND_REASON_BYTES);
        assert_eq!(command.state(), NoticeCommandState::Disabled);
    }
    assert!(bounded.retained_text_bytes() <= NOTICE_RECORD_TEXT_BYTES);
    let debug = format!("{bounded:?}");
    assert!(!debug.contains("秘密"));
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let mut input = record(NoticeKind::Warning, "");
    input.content = bounded;
    admitted(&mut arbiter, input);
    assert!(!format!("{:?}", arbiter.diagnostics()).contains("秘密"));
}

#[test]
fn command_capacity_and_identity_validation_preserve_bounded_contract() {
    let one = NoticeCommand::enabled(NoticeCommandId::new(7), "one");
    let duplicate = NoticeCommand::loading(NoticeCommandId::new(7), "changed");
    assert_eq!(
        content("", NoticeDismissal::Dismissible).with_commands(&[one.clone(), duplicate]),
        Err(NoticeRejection::DuplicateCommand)
    );
    assert_eq!(
        content("", NoticeDismissal::Dismissible)
            .with_commands(&vec![one; NOTICE_COMMAND_CAPACITY + 1]),
        Err(NoticeRejection::TooManyCommands)
    );
    let exact = "x".repeat(NOTICE_TITLE_BYTES);
    assert!(
        !content(&exact, NoticeDismissal::Dismissible)
            .title()
            .is_truncated()
    );
}

#[test]
fn repeated_updates_removals_and_disposal_release_all_retained_records_and_text() {
    let mut arbiter = MainWindowNoticeArbiter::new(window(1));
    let maximum = "x".repeat(NOTICE_DETAIL_BYTES * 2);
    for _ in 0..1_000 {
        let token = admitted(&mut arbiter, record(NoticeKind::Warning, &maximum));
        let latest = arbiter
            .update(
                &token,
                u64::MAX,
                content(&maximum, NoticeDismissal::Dismissible),
            )
            .unwrap();
        assert!(arbiter.diagnostics().retained_text_bytes <= NOTICE_RECORD_TEXT_BYTES);
        assert_eq!(
            arbiter.update(
                &latest,
                u64::MAX,
                content("old", NoticeDismissal::Dismissible)
            ),
            Err(NoticeRejection::StaleRevision)
        );
        arbiter.remove(&latest).unwrap();
        assert_eq!(arbiter.diagnostics().retained_records, 0);
        assert_eq!(arbiter.diagnostics().retained_text_bytes, 0);
    }
    let stale = admitted(&mut arbiter, record(NoticeKind::HomeFailure, &maximum));
    fill_information(&mut arbiter);
    arbiter.dispose();
    arbiter.dispose();
    assert!(arbiter.active().is_none());
    assert_eq!(arbiter.diagnostics().retained_records, 0);
    assert_eq!(arbiter.diagnostics().retained_text_bytes, 0);
    assert!(arbiter.diagnostics().disposed);
    assert_eq!(arbiter.remove(&stale), Err(NoticeRejection::Disposed));
    assert_eq!(
        arbiter.admit(record(NoticeKind::Warning, "late")),
        NoticeAdmission::Rejected(NoticeRejection::Disposed)
    );
}
