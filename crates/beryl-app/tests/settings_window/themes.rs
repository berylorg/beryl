use super::support;
use beryl_app::settings_window::SettingsWindowController;
use beryl_state::SettingKey;
use gpui_settings_window::SettingsFooterCommand;

#[gpui::test]
fn theme_selection_is_qualified_before_apply_and_an_older_bound_document_remains_independent(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, || support::Fixture::with_themes(2));
    let (owner, appearance) = fixture.mount(cx);
    let mut fixture = fixture.attach_appearance(cx, &appearance);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    let view = window.entity(cx).unwrap();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"themes".into())
                .and_then(|page| page.paged_split_source())
                .is_some()
        })
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.demand_page_split_range_for_test(0..2, cx)
        })
        .unwrap();
    support::wait_for(cx, |cx| {
        window
            .window_handle()
            .update(cx, |view, _, cx| {
                view.request_page_split_item_for_test("themes".into(), "theme-0".into(), cx)
            })
            .unwrap()
    });
    cx.run_until_parked();
    support::wait_for(cx, |cx| {
        support::page_action(cx, &window, "themes", "theme.edit")
    });
    let (field, source) = view.read_with(cx, |view, _| {
        let row = view
            .model()
            .selected_rows()
            .iter()
            .find(|row| row.field_id().as_str().starts_with("theme.property."))
            .unwrap();
        (row.field_id().clone(), row.value().to_owned())
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.select_choice_for_test(
                field.clone(),
                if source == "value" {
                    "fallback".into()
                } else {
                    "value".into()
                },
                cx
            ))
        })
        .unwrap();
    cx.run_until_parked();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .selected_page()
                .actions()
                .iter()
                .any(|action| action.action_id().as_str() == "theme.save" && action.is_enabled())
        })
    });
    let staged = view.read_with(cx, |view, _| {
        view.model().field_value(&field).unwrap().to_owned()
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_page_navigation_for_test("themes".into(), cx))
        })
        .unwrap();
    cx.run_until_parked();
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.demand_page_split_range_for_test(0..2, cx)
        })
        .unwrap();
    support::wait_for(cx, |cx| {
        window
            .window_handle()
            .update(cx, |view, _, cx| {
                view.request_page_split_item_for_test("themes".into(), "theme-1".into(), cx)
            })
            .unwrap()
    });
    cx.run_until_parked();
    support::route(cx, &window, "operations");
    support::route(cx, &window, "themes");
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .selected_page()
                .actions()
                .iter()
                .any(|action| action.action_id().as_str() == "theme.retry" && action.is_enabled())
        })
    });
    assert!(support::page_action(cx, &window, "themes", "theme.retry"));
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model().selected_page().actions().iter().any(|action| {
                action.action_id().as_str() == "theme.activate" && action.is_enabled()
            })
        })
    });
    let baseline = fixture.service.snapshot().unwrap().identity();
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_page_action_for_test("themes".into(), "theme.activate".into(), cx))
        })
        .unwrap();
    assert!(!view.read_with(cx, |view, _| {
        view.model()
            .footer_availability(SettingsFooterCommand::Apply)
            .is_enabled()
    }));
    assert!(fixture.service.pending_operation().is_none());
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .footer_availability(SettingsFooterCommand::Apply)
                .is_enabled()
        })
    });
    assert_eq!(fixture.service.snapshot().unwrap().identity(), baseline);
    window
        .window_handle()
        .update(cx, |view, _, cx| view.apply_for_test(cx))
        .unwrap();
    let mut proof = None;
    support::wait_for(cx, |cx| {
        proof = cx.update(|app| owner.update(app, |owner, _| owner.take_confirmed_theme()));
        proof.is_some()
    });
    fixture = fixture.consume_settings(cx, &owner, proof.unwrap());
    assert_eq!(
        fixture
            .service
            .snapshot()
            .unwrap()
            .row(SettingKey::ActiveThemeId)
            .value,
        "theme-1"
    );
    assert!(view.read_with(cx, |view, _| {
        view.model()
            .selected_rows()
            .iter()
            .all(|row| row.field_id().as_str() != "theme.document.commands")
    }));
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_page_navigation_for_test("theme-editor".into(), cx))
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view
            .model()
            .field_value(&field)
            .unwrap()
            .to_owned()),
        staged
    );
    assert!(view.read_with(cx, |view, _| {
        view.model()
            .field_availability(&field)
            .unwrap()
            .is_available()
    }));
    let previous = fixture.runtime.current().unwrap();
    assert!(support::page_action(
        cx,
        &window,
        "theme-editor",
        "theme.save"
    ));
    let completion = support::committed_repository_completion(cx, &fixture);
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"theme-editor".into())
                .unwrap()
                .actions()
                .iter()
                .all(|action| action.action_id().as_str() != "theme.save")
        })
    });
    fixture = fixture.consume_repository_completion(cx, &owner, completion);
    assert!(std::sync::Arc::ptr_eq(
        &previous,
        &fixture.runtime.current().unwrap()
    ));
    let authentication_started = std::time::Instant::now();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            let availability = view.model().field_availability(&field).unwrap();
            assert!(
                authentication_started.elapsed() < std::time::Duration::from_secs(19)
                    || availability.is_available(),
                "saved original source did not authenticate: availability={:?}, rows={:?}",
                availability,
                view.model()
                    .selected_rows()
                    .iter()
                    .map(|row| (row.field_id().as_str(), row.error(), row.subtext()))
                    .collect::<Vec<_>>()
            );
            availability.is_available()
        })
    });
    assert_eq!(
        fixture
            .service
            .snapshot()
            .unwrap()
            .row(SettingKey::ActiveThemeId)
            .value,
        "theme-1"
    );
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn committed_save_is_clean_when_its_original_source_retires_before_document_reread(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, || support::Fixture::with_themes(1));
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    let view = window.entity(cx).unwrap();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"themes".into())
                .and_then(|page| page.paged_split_source())
                .is_some()
        })
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.demand_page_split_range_for_test(0..1, cx)
        })
        .unwrap();
    support::wait_for(cx, |cx| {
        window
            .window_handle()
            .update(cx, |view, _, cx| {
                view.request_page_split_item_for_test("themes".into(), "theme-0".into(), cx)
            })
            .unwrap()
    });
    cx.run_until_parked();
    support::wait_for(cx, |cx| {
        support::page_action(cx, &window, "themes", "theme.edit")
    });
    let (field, source) = view.read_with(cx, |view, _| {
        let row = view
            .model()
            .selected_rows()
            .iter()
            .find(|row| row.field_id().as_str().starts_with("theme.property."))
            .unwrap();
        (row.field_id().clone(), row.value().to_owned())
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.select_choice_for_test(
                field.clone(),
                if source == "value" {
                    "fallback".into()
                } else {
                    "value".into()
                },
                cx
            ))
        })
        .unwrap();
    cx.run_until_parked();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .selected_page()
                .actions()
                .iter()
                .any(|action| action.action_id().as_str() == "theme.save" && action.is_enabled())
        })
    });
    let saved_value = view.read_with(cx, |view, _| {
        view.model().field_value(&field).unwrap().to_owned()
    });
    assert!(support::page_action(
        cx,
        &window,
        "theme-editor",
        "theme.save"
    ));
    let (fixture, retirement) = support::on_worker(cx, move || {
        let mut fixture = fixture;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let completion = loop {
            if let Some(completion) = fixture.broker.take_completion() {
                break completion;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "original save did not complete"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(matches!(
            completion.repository.as_deref(),
            Some(beryl_state::ThemeRepositoryOperationOutcome::Committed { .. })
        ));
        let mut retirement = fixture.broker.retire();
        assert!(retirement.completion.is_none());
        retirement.completion = Some(completion);
        (fixture, retirement)
    });
    cx.update(|app| {
        owner.update(app, |owner, cx| {
            owner.observe_theme_retirement(&retirement, cx)
        })
    });
    assert!(!owner.read_with(cx, |owner, _| owner.exit_is_gated()));
    view.read_with(cx, |view, _| {
        let page = view.model().page(&"theme-editor".into()).unwrap();
        assert!(
            page.actions()
                .iter()
                .all(|action| action.action_id().as_str() != "theme.save"
                    && action.action_id().as_str() != "theme.save-as")
        );
        assert!(
            page.rows()
                .iter()
                .filter(|row| row.field_id().as_str().starts_with("theme.property."))
                .all(|row| !row.is_modified())
        );
        assert_eq!(view.model().field_value(&field), Some(saved_value.as_str()));
        assert!(
            !view
                .model()
                .field_availability(&field)
                .unwrap()
                .is_available()
        );
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| {
        view.model()
            .page(&"theme-editor".into())
            .unwrap()
            .actions()
            .iter()
            .all(|action| action.action_id().as_str() != "theme.save")
    }));
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    drop(retirement);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn retiring_the_original_theme_broker_settles_its_window_gate_without_transferring_old_execution(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, || support::Fixture::with_themes(1));
    let (owner, _appearance) = fixture.mount(cx);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    let view = window.entity(cx).unwrap();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"themes".into())
                .and_then(|page| page.paged_split_source())
                .is_some()
        })
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.demand_page_split_range_for_test(0..1, cx)
        })
        .unwrap();
    support::wait_for(cx, |cx| {
        window
            .window_handle()
            .update(cx, |view, _, cx| {
                view.request_page_split_item_for_test("themes".into(), "theme-0".into(), cx)
            })
            .unwrap()
    });
    cx.run_until_parked();
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.replace_field_text_for_test(
                &"theme.rename-name".into(),
                "Renamed active theme",
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    assert!(support::page_action(cx, &window, "themes", "theme.rename"));
    assert!(owner.read_with(cx, |owner, _| owner.exit_is_gated()));
    let (fixture, retirement) = support::on_worker(cx, move || {
        let mut fixture = fixture;
        let retirement = fixture.broker.retire();
        (fixture, retirement)
    });
    let completion = retirement.completion.as_ref().expect(
        "queued or executed original operation retains its exact retirement classification",
    );
    assert!(
        completion.not_started.is_some()
            || matches!(
                completion.repository.as_deref(),
                Some(beryl_state::ThemeRepositoryOperationOutcome::Committed { .. })
            )
    );
    cx.update(|app| {
        owner.update(app, |owner, cx| {
            owner.observe_theme_retirement(&retirement, cx)
        })
    });
    assert!(!owner.read_with(cx, |owner, _| owner.exit_is_gated()));
    assert_eq!(
        fixture
            .service
            .snapshot()
            .unwrap()
            .row(SettingKey::ActiveThemeId)
            .value,
        "theme-0"
    );
    let (fixture, documents) = support::on_worker(cx, move || {
        let documents = fixture.documents();
        (fixture, documents)
    });
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].document().name(), Some("Theme 0"));
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    drop(retirement);
    support::on_worker(cx, move || fixture.dispose());
}

#[gpui::test]
fn installed_themes_use_bounded_pages_and_save_as_preserves_original_draft_selection_and_activation(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = support::on_worker(cx, || support::Fixture::with_themes(40));
    let (owner, appearance) = fixture.mount(cx);
    let mut fixture = fixture.attach_appearance(cx, &appearance);
    cx.update(|app| SettingsWindowController::command(&owner).open(app).unwrap());
    let window = owner.read_with(cx, |owner, _| owner.native_window().unwrap().clone());
    let view = window.entity(cx).unwrap();
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"themes".into())
                .and_then(|page| page.paged_split_source())
                .is_some_and(|source| source.logical_item_count() == 40)
        })
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.demand_page_split_range_for_test(0..32, cx)
        })
        .unwrap();
    support::draw(cx, &window);
    let metrics = view.read_with(cx, |view, app| {
        view.page_split_render_metrics_for_test(app).unwrap()
    });
    assert_eq!(metrics.0, 40);
    assert!(metrics.2 - metrics.1 <= 12);
    support::wait_for(cx, |cx| {
        window
            .window_handle()
            .update(cx, |view, _, cx| {
                view.request_page_split_item_for_test("themes".into(), "theme-0".into(), cx)
            })
            .unwrap()
    });
    cx.run_until_parked();
    support::wait_for(cx, |cx| {
        support::page_action(cx, &window, "themes", "theme.edit")
    });
    support::draw(cx, &window);
    let navigator = owner.read_with(cx, |owner, _| owner.theme_role_navigator().unwrap());
    let old_lease = navigator.read_with(cx, |navigator, _| navigator.scroll_lease().unwrap());
    let (source_field, source) = view.read_with(cx, |view, _| {
        let row = view
            .model()
            .selected_rows()
            .iter()
            .find(|row| row.field_id().as_str().starts_with("theme.property."))
            .unwrap();
        (row.field_id().clone(), row.value().to_owned())
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.select_choice_for_test(
                source_field.clone(),
                if source == "value" {
                    "fallback".into()
                } else {
                    "value".into()
                },
                cx
            ))
        })
        .unwrap();
    cx.run_until_parked();
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            view.replace_field_text_for_test(&"theme.save-as-name".into(), "Saved copy", cx)
        })
        .unwrap();
    cx.run_until_parked();
    let original_source = view.read_with(cx, |view, _| {
        view.model().field_value(&source_field).unwrap().to_owned()
    });
    let before_binding = fixture.service.snapshot().unwrap().identity();
    let (returned, original) = support::on_worker(cx, move || {
        let docs = fixture.documents();
        let original = docs
            .iter()
            .find(|document| document.identity().theme_id().as_str() == "theme-0")
            .unwrap()
            .clone();
        (fixture, original)
    });
    fixture = returned;
    let old_key = view.read_with(cx, |view, _| {
        view.model()
            .page(&"themes".into())
            .unwrap()
            .paged_split_source()
            .unwrap()
            .key()
            .clone()
    });
    support::wait_for(cx, |cx| {
        support::page_action(cx, &window, "theme-editor", "theme.save-as")
    });
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"themes".into())
                .and_then(|page| page.paged_split_source())
                .is_some_and(|source| source.key() != &old_key && source.logical_item_count() == 41)
        })
    });
    fixture = fixture.consume_repository(cx, &owner);
    support::settle(cx, &owner);
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"theme-editor".into())
                .unwrap()
                .actions()
                .iter()
                .any(|action| action.action_id().as_str() == "theme.save-as" && action.is_enabled())
        })
    });
    assert_eq!(
        view.read_with(cx, |view, _| view
            .model()
            .field_value(&source_field)
            .unwrap()
            .to_owned()),
        original_source
    );
    assert_eq!(
        view.read_with(cx, |view, _| view
            .model()
            .field_value(&"theme.selected.id".into())
            .unwrap()
            .to_owned()),
        "theme-0"
    );
    assert!(!view.read_with(cx, |view, _| {
        view.model()
            .footer_availability(SettingsFooterCommand::Apply)
            .is_enabled()
    }));
    assert_eq!(
        fixture.service.snapshot().unwrap().identity(),
        before_binding
    );
    assert_eq!(
        fixture
            .service
            .snapshot()
            .unwrap()
            .row(SettingKey::ActiveThemeId)
            .value,
        "theme-0"
    );
    let (returned, documents) = support::on_worker(cx, move || {
        assert!(fixture.has_installed_name("Saved copy"));
        let documents = fixture.documents();
        (fixture, documents)
    });
    fixture = returned;
    assert_eq!(documents.len(), 41);
    assert_eq!(
        documents
            .iter()
            .find(|document| document.identity().theme_id().as_str() == "theme-0")
            .unwrap()
            .identity()
            .digest(),
        original.identity().digest()
    );
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_page_navigation_for_test("themes".into(), cx))
        })
        .unwrap();
    cx.run_until_parked();
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_row_action_for_test(
                "theme.document.commands".into(),
                "theme.save".into(),
                cx
            ))
        })
        .unwrap();
    cx.run_until_parked();
    let completion = support::committed_repository_completion(cx, &fixture);
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"theme-editor".into())
                .unwrap()
                .actions()
                .iter()
                .all(|action| action.action_id().as_str() != "theme.save")
        })
    });
    fixture = fixture.consume_repository_completion(cx, &owner, completion);
    support::settle(cx, &owner);
    support::wait_for(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.model()
                .page(&"theme-editor".into())
                .unwrap()
                .actions()
                .iter()
                .all(|action| action.action_id().as_str() != "theme.save")
        })
    });
    window
        .window_handle()
        .update(cx, |view, _, cx| {
            assert!(view.request_page_navigation_for_test("themes".into(), cx))
        })
        .unwrap();
    cx.run_until_parked();
    assert!(navigator.read_with(cx, |navigator, _| navigator.scroll_lease().is_none()));
    let role = beryl_state::canonical_theme_schema()
        .roles()
        .values()
        .find(|role| role.static_parent().is_none())
        .unwrap()
        .id()
        .clone();
    window
        .window_handle()
        .update(cx, |_, window, cx| {
            assert!(!navigator.update(cx, |navigator, cx| {
                navigator.activate_role(old_lease, None, role, window, cx)
            }))
        })
        .unwrap();
    cx.update(|app| owner.update(app, |owner, cx| assert!(owner.shutdown(cx).unwrap())));
    drop(owner);
    support::on_worker(cx, move || fixture.dispose());
}
