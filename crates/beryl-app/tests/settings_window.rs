use beryl_app::settings_window::SettingsWindowController;
use beryl_state::SettingKey;
use gpui::EntityInputHandler as _;
use gpui_settings_window::{SettingsFieldValidity, SettingsFooterCommand};

#[path = "settings_window/support.rs"]
mod support;
#[path = "settings_window/themes.rs"]
mod themes;

#[gpui::test]
fn replacing_service_preserves_stale_dirty_values_read_only_until_explicit_discard(
    cx: &mut gpui::TestAppContext,
) {
    let mut fixture = support::on_worker(cx, support::Fixture::new);
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    support::route(cx, &window, "agent");
    support::edit(
        cx,
        &window,
        SettingKey::DeveloperInstructions,
        "Old exact draft",
    );
    let input = window.entity(cx).unwrap().read_with(cx, |view, app| {
        view.text_input_for_field_for_test(
            &SettingKey::DeveloperInstructions.stable_id().into(),
            app,
        )
        .unwrap()
    });
    let old = fixture.service.clone();
    let source = fixture.store.service_reference();
    let settings = fixture.state.settings();
    let process = fixture.owner.clone();
    let theme = fixture.runtime.settings_source().unwrap();
    let fresh = support::on_worker(cx, move || {
        old.retire().unwrap();
        let service = std::sync::Arc::new(
            beryl_app::settings_service::SettingsService::prepare(source, settings, process, 2)
                .unwrap(),
        );
        service.bind_theme_source(theme).unwrap();
        service
    });
    let themes = fixture.broker.reader();
    cx.update(|app| {
        owner.update(app, |owner, cx| {
            owner.replace_service(fresh.clone(), themes, cx).unwrap()
        })
    });
    fixture.service = fresh;
    assert!(input.read_with(cx, |input, _| input.state().is_read_only()));
    window
        .window_handle()
        .update(cx, |view, window, cx| {
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "late old callback", window, cx)
            });
            view.apply_for_test(cx);
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "Old exact draft"
    );
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        ""
    );
    window
        .window_handle()
        .update(cx, |view, _, cx| view.cancel_for_test(cx))
        .unwrap();
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    assert!(!input.read_with(cx, |input, _| input.state().is_read_only()));
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn reset_restores_only_its_active_row_and_cancel_discards_the_remaining_window_draft(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, support::Fixture::new);
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    let before = fixture.service.snapshot().unwrap();
    support::route(cx, &window, "operations");
    support::edit(cx, &window, SettingKey::ContextCompactionTimeout, "120");
    support::edit(cx, &window, SettingKey::DraftAutosaveInterval, "60");
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_row_action_for_test(
                SettingKey::DraftAutosaveInterval.stable_id().into(),
                "reset".into(),
                cx
            ))
        })
        .unwrap();
    cx.run_until_parked();
    let view = window.entity(cx).unwrap();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.model()
                .field_value(&SettingKey::DraftAutosaveInterval.stable_id().into()),
            Some(before.row(SettingKey::DraftAutosaveInterval).value.as_str())
        );
        assert_eq!(
            view.model()
                .field_value(&SettingKey::ContextCompactionTimeout.stable_id().into()),
            Some("120")
        );
        assert!(
            view.model()
                .footer_availability(SettingsFooterCommand::Apply)
                .is_enabled()
        );
    });
    window
        .window_handle()
        .update(cx, |view, window, cx| {
            view.request_close_for_test(window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(!window.is_visible(cx).unwrap());
    assert_eq!(
        fixture.service.snapshot().unwrap().identity(),
        before.identity()
    );
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.model()
                .field_value(&SettingKey::ContextCompactionTimeout.stable_id().into()),
            Some(
                before
                    .row(SettingKey::ContextCompactionTimeout)
                    .value
                    .as_str()
            )
        )
    });
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn appearance_flight_gates_retained_editors_all_commands_and_native_close_without_replacing_input(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, support::Fixture::new);
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    support::route(cx, &window, "agent");
    support::edit(
        cx,
        &window,
        SettingKey::DeveloperInstructions,
        "Retained draft",
    );
    let view = window.entity(cx).unwrap();
    let input = view.read_with(cx, |view, app| {
        view.text_input_for_field_for_test(
            &SettingKey::DeveloperInstructions.stable_id().into(),
            app,
        )
        .unwrap()
    });
    cx.update(|app| owner.update(app, |owner, cx| owner.set_appearance_busy(true, cx)));
    assert!(input.read_with(cx, |input, _| input.state().is_read_only()));
    for _ in 0..2 {
        window
            .window_handle()
            .update(cx, |view, window, cx| {
                view.apply_for_test(cx);
                view.accept_for_test(cx);
                view.cancel_for_test(cx);
                assert!(!view.request_row_action_for_test(
                    SettingKey::DeveloperInstructions.stable_id().into(),
                    "reset".into(),
                    cx
                ));
                assert!(!view.request_page_navigation_for_test("operations".into(), cx));
                assert!(!view.request_close_for_test(window, cx));
                input.update(cx, |input, cx| {
                    input.replace_text_in_range(None, "stale mutation", window, cx)
                });
            })
            .unwrap();
        cx.update(|app| assert!(!window.hide(app).unwrap()));
        cx.run_until_parked();
    }
    assert!(window.is_visible(cx).unwrap());
    assert_eq!(
        input.read_with(cx, |input, _| input.text().to_owned()),
        "Retained draft"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .model()
            .selected_page_id()
            .as_str()
            .to_owned()),
        "agent"
    );
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        ""
    );
    cx.update(|app| owner.update(app, |owner, cx| owner.set_appearance_busy(false, cx)));
    assert!(!input.read_with(cx, |input, _| input.state().is_read_only()));
    assert_eq!(
        input.entity_id(),
        view.read_with(cx, |view, app| view
            .text_input_for_field_for_test(
                &SettingKey::DeveloperInstructions.stable_id().into(),
                app
            )
            .unwrap()
            .entity_id())
    );
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn native_settings_preserves_one_whole_draft_and_applies_all_modified_rows_atomically(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, support::Fixture::new);
    let (owner, _appearance) = fixture.mount(cx);
    let command = SettingsWindowController::command(&owner);
    assert!(command.is_alive());
    cx.update(|app| command.open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    assert_eq!(
        window.entity(cx).unwrap().read_with(cx, |view, _| view
            .model()
            .sections()
            .iter()
            .map(|section| section.section_id().as_str().to_owned())
            .collect::<Vec<_>>()),
        ["themes", "operations", "notifications", "agent"]
    );
    let before = fixture.service.snapshot().unwrap().identity();
    support::route(cx, &window, "operations");
    support::edit(cx, &window, SettingKey::DraftAutosaveInterval, "60");
    support::route(cx, &window, "agent");
    support::edit(
        cx,
        &window,
        SettingKey::DeveloperInstructions,
        &"x".repeat(60 * 1024 + 1),
    );
    let view = window.entity(cx).unwrap();
    assert!(!view.read_with(cx, |view, _| {
        view.model()
            .footer_availability(SettingsFooterCommand::Apply)
            .is_enabled()
    }));
    window
        .window_handle()
        .update(cx, |view, _, cx| view.apply_for_test(cx))
        .unwrap();
    assert_eq!(fixture.service.snapshot().unwrap().identity(), before);
    support::edit(
        cx,
        &window,
        SettingKey::DeveloperInstructions,
        "Exact instructions",
    );
    window
        .window_handle()
        .update(cx, |view, _, cx| view.apply_for_test(cx))
        .unwrap();
    support::settle(cx, &owner);
    let applied = fixture.service.applied_reader().snapshot().unwrap();
    assert_eq!(applied.draft_autosave_interval().unwrap().as_secs(), 60);
    assert_eq!(
        applied.developer_instructions().unwrap(),
        "Exact instructions"
    );
    assert!(window.is_visible(cx).unwrap());
    window
        .window_handle()
        .update(cx, |view, _, cx| view.accept_for_test(cx))
        .unwrap();
    assert!(!window.is_visible(cx).unwrap());
    let committed = fixture.service.snapshot().unwrap().identity();
    cx.update(|app| command.open(app).unwrap());
    window
        .window_handle()
        .update(cx, |view, _, cx| view.cancel_for_test(cx))
        .unwrap();
    assert_eq!(fixture.service.snapshot().unwrap().identity(), committed);
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn developer_input_hard_cap_refuses_platform_mutation_without_losing_exact_selection_or_history(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, support::Fixture::new);
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    support::route(cx, &window, "agent");
    support::edit(
        cx,
        &window,
        SettingKey::DeveloperInstructions,
        &"é".repeat(32 * 1024),
    );
    let input = window.entity(cx).unwrap().read_with(cx, |view, app| {
        view.text_input_for_field_for_test(
            &SettingKey::DeveloperInstructions.stable_id().into(),
            app,
        )
        .unwrap()
    });
    cx.update(|app| {
        input.update(app, |input, cx| {
            input.set_selection(input.text().len()..input.text().len(), true, cx);
        })
    });
    let before = input.read_with(cx, |input, _| {
        (
            input.text().to_owned(),
            input.selection(),
            input.state().selection_reversed(),
            input.retained_counts(),
            input.scroll_offset(),
        )
    });
    window
        .window_handle()
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, "é", window, cx)
            })
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        input.read_with(cx, |input, _| (
            input.text().to_owned(),
            input.selection(),
            input.state().selection_reversed(),
            input.retained_counts(),
            input.scroll_offset()
        )),
        before
    );
    let view = window.entity(cx).unwrap();
    assert_eq!(
        view.read_with(cx, |view, _| view
            .model()
            .field_validity(&SettingKey::DeveloperInstructions.stable_id().into())),
        Some(SettingsFieldValidity::Invalid)
    );
    assert!(view.read_with(cx, |view, _| {
        view.model()
            .field_error(&SettingKey::DeveloperInstructions.stable_id().into())
            .unwrap()
            .contains("refused")
    }));
    assert_eq!(
        fixture
            .service
            .applied_reader()
            .snapshot()
            .unwrap()
            .developer_instructions()
            .unwrap(),
        ""
    );
    window
        .window_handle()
        .update(cx, |view, _, cx| view.cancel_for_test(cx))
        .unwrap();
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), "");
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}
