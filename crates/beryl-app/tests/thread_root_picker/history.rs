use super::*;

fn visit(
    picker: &gpui::Entity<ThreadRootPicker>,
    key: &str,
    revision: u64,
    initial_focus: Option<usize>,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.replace_collection(
                PickerCollectionKey(key.into()),
                revision,
                100_000,
                initial_focus,
                window,
                cx,
            );
        })
    });
    admit(picker, cx);
}

fn focus(
    picker: &gpui::Entity<ThreadRootPicker>,
    position: usize,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, app| {
        picker.update(app, |picker, cx| picker.focus_row(position, window, cx))
    });
    draw(cx);
}

#[gpui::test]
fn collection_history_counts_current_and_evicts_seventeenth_scope_without_selection_or_activation(
    cx: &mut gpui::TestAppContext,
) {
    assert_eq!(PICKER_COLLECTION_HISTORY_CAPACITY, 16);
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        1
    );
    let selected = PickerRowKey("row-2".into());
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            picker.activate(&selected, cx);
            picker.set_selection_eligibility(&selected, None, cx);
            picker.focus_search(window, cx);
        })
    });
    cx.simulate_input("search belonging to the old scope");
    admit(&picker, cx);
    focus(&picker, 24, cx);
    let original = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(original.focused_key, Some(PickerRowKey("row-24".into())));
    assert!(original.scroll_offset > 0.);
    let (events, _subscription) = collect(&picker, cx);
    for scope in 1..=15 {
        visit(
            &picker,
            &format!("scope-{scope}"),
            100 + scope as u64,
            Some(0),
            cx,
        );
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.retained_collection_count()),
            scope + 1
        );
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.selected_key().cloned()),
            Some(selected.clone())
        );
    }
    visit(&picker, "scope-16", 116, Some(0), cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    visit(&picker, "roots-all", 117, Some(0), cx);
    let reentered = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    assert_eq!(reentered.focused_key, Some(PickerRowKey("row-0".into())));
    assert_eq!(reentered.scroll_offset, 0.);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.selected_key().cloned()),
        Some(selected.clone())
    );
    assert!(!picker.read_with(cx, |picker, _| {
        picker.confirmation_state().unwrap().can_dispatch()
    }));
    cx.update(|_, app| {
        picker.update(app, |picker, cx| {
            picker.set_selection_eligibility(&selected, None, cx);
            assert!(picker.confirmation_state().unwrap().can_dispatch());
        })
    });
    for scope in 17..=80 {
        visit(
            &picker,
            &format!("scope-{scope}"),
            200 + scope as u64,
            Some(0),
            cx,
        );
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.retained_collection_count()),
            16
        );
    }
    assert!(!events.borrow().iter().any(|event| matches!(
        event,
        PickerEvent::Activate(_) | PickerEvent::Command(_) | PickerEvent::SelectionChanged(_)
    )));
}

#[gpui::test]
fn returning_old_scope_refreshes_recency_and_same_scope_revisions_do_not_consume_history(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    focus(&picker, 10, cx);
    let initial = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert!(initial.scroll_offset > 0.);
    visit(&picker, "scope-1", 2, Some(0), cx);
    focus(&picker, 11, cx);
    for scope in 2..=15 {
        visit(
            &picker,
            &format!("scope-{scope}"),
            10 + scope as u64,
            Some(0),
            cx,
        );
    }
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    let (events, _subscription) = collect(&picker, cx);
    visit(&picker, "roots-all", 50, None, cx);
    let returned = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(returned.focused_key, initial.focused_key);
    assert_eq!(returned.scroll_offset, initial.scroll_offset);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    for revision in 51..=65 {
        visit(&picker, "roots-all", revision, None, cx);
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.retained_collection_count()),
            16
        );
    }
    cx.update(|window, app| picker.update(app, |picker, cx| picker.focus_search(window, cx)));
    cx.simulate_input("scope query");
    admit(&picker, cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    cx.update(|_, app| picker.update(app, |picker, cx| picker.clear_search(cx)));
    admit(&picker, cx);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    focus(&picker, 10, cx);
    visit(&picker, "scope-16", 100, Some(0), cx);
    visit(&picker, "roots-all", 101, None, cx);
    let protected = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(protected.focused_key, initial.focused_key);
    assert_eq!(protected.scroll_offset, initial.scroll_offset);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.query_text().to_owned()),
        ""
    );
    visit(&picker, "scope-1", 102, Some(0), cx);
    let evicted = picker.read_with(cx, |picker, _| picker.diagnostics());
    assert_eq!(evicted.focused_key, Some(PickerRowKey("row-0".into())));
    assert_eq!(evicted.scroll_offset, 0.);
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    assert!(!events.borrow().iter().any(|event| matches!(
        event,
        PickerEvent::Activate(_) | PickerEvent::Command(_) | PickerEvent::SelectionChanged(_)
    )));
}

#[gpui::test]
fn closing_picker_discards_scope_history_and_another_instance_starts_with_only_current_scope(
    cx: &mut gpui::TestAppContext,
) {
    let (picker, cx) = mount(cx);
    admit(&picker, cx);
    focus(&picker, 24, cx);
    for scope in 1..=16 {
        visit(
            &picker,
            &format!("scope-{scope}"),
            scope as u64 + 1,
            Some(0),
            cx,
        );
    }
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        16
    );
    cx.update(|window, app| picker.update(app, |picker, cx| picker.dismiss(window, cx)));
    assert_eq!(
        picker.read_with(cx, |picker, _| picker.retained_collection_count()),
        0
    );
    let replacement = cx.update(|window, app| {
        app.new(|cx| {
            ThreadRootPicker::new(
                ThreadRootPickerConfig {
                    title: "New thread".into(),
                    helper: "Choose a root".into(),
                    heading: "ROOTS FOR ALL RUNTIMES".into(),
                    empty_text: "No roots".into(),
                    search_placeholder: "Search roots".into(),
                    owner_focus: cx.focus_handle(),
                    appearance: None,
                    style: ThreadRootPickerStyle::default(),
                    scrollbar_style: gpui_scrollbar::ScrollbarStyle::default(),
                },
                PickerCollectionKey("roots-all".into()),
                1,
                100_000,
                window,
                cx,
            )
        })
    });
    assert_eq!(
        replacement.read_with(cx, |picker, _| picker.retained_collection_count()),
        1
    );
    assert_eq!(
        replacement.read_with(cx, |picker, _| picker.focused_key().cloned()),
        None
    );
    assert_eq!(
        replacement.read_with(cx, |picker, _| picker.diagnostics().scroll_offset),
        0.
    );
}
