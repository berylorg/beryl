use super::support::editor_snapshot;
use super::*;
use beryl_app::main_window::{MainWindowShell, MainWindowShellHostFailure};
use beryl_app::theme_runtime::{
    AppearancePublicationTarget, PreparedPreviewAppearance, PreviewCandidateIdentity,
    PreviewSource, PreviewSourceIdentity,
};

#[path = "../support/window_placement.rs"]
mod placement_support;

#[cfg(target_os = "windows")]
#[gpui::test]
fn acquired_startup_shell_applies_only_its_exact_prepared_geometry(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    for foreign in [false, true] {
        let (mut fixture, prepared) = support::join(
            support::worker(|| {
                let mut fixture = ShellFixture::new(151);
                let prepared = fixture.prepare();
                (fixture, prepared)
            }),
            cx,
        );
        let owner = fixture.owner(cx);
        let native = placement_support::prepare(
            beryl_model::WindowId::from_bytes([if foreign { 152 } else { 151 }; 16]),
            &placement(),
        );
        let expected = native.gpui_window_bounds().unwrap();
        let before = cx.update(|app| app.windows().len());
        let result = cx.update(|app| {
            GpuiMainWindowShellHost::new(app, owner)
                .with_prepared_placement(native)
                .construct_hidden(prepared)
        });
        let unpublished = if foreign {
            let Err(MainWindowShellHostFailure::BeforeConstruction { prepared, .. }) = result
            else {
                panic!("foreign placement must return original acquired preparation")
            };
            assert_eq!(cx.update(|app| app.windows().len()), before);
            prepared.into_unpublished()
        } else {
            let shell = result.unwrap_or_else(|_| panic!("placed startup acquisition"));
            shell
                .window()
                .update(cx, |_, window, _| {
                    assert_eq!(window.window_bounds(), expected)
                })
                .unwrap();
            assert!(!cx.window_visibility(shell.window().into()).is_visible);
            cx.update(|app| shell.close_before_publication(app))
                .unwrap_or_else(|_| panic!("close placed acquired shell"))
        };
        let occupancy = support::join(
            support::worker(move || {
                let beryl_app::main_window::MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } = unpublished.prepare_abandonment(&fixture.service, CommandCancellation::new()) else { panic!("exact placement cleanup") };
                assert!(matches!(
                    abandonment.abandon(&fixture.service, CommandCancellation::new()),
                    beryl_app::main_window::MainWindowShellAbandonmentOutcome::Committed { .. }
                ));
                fixture.process.main_window_occupancy()
            }),
            cx,
        );
        assert_eq!(occupancy, 0);
    }
}

fn preview(
    coordinator: &mut AppearanceCoordinator,
    source: u64,
) -> Result<(), beryl_app::theme_runtime::PreviewPublicationError> {
    let request = coordinator.begin_preview(
        PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(source).unwrap()),
        PreviewCandidateIdentity::Digest(beryl_state::ThemeDocumentDigest::from_bytes(
            [source as u8; 32],
        )),
    )?;
    let candidate = request.candidate().clone();
    coordinator.publish_preview(
        request,
        PreparedPreviewAppearance::new(candidate, coordinator.current().prepared().clone()),
    )?;
    Ok(())
}

#[gpui::test]
fn two_shells_use_current_preview_and_later_atomic_adoption_rejects_released_editor(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, first, second) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(71);
            let first = fixture.prepare();
            let second = fixture.additional(81);
            preview(fixture.coordinator.as_mut().unwrap(), 1).unwrap();
            fixture.appearance = fixture.coordinator.as_ref().unwrap().current();
            (fixture, first, second)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shells = cx.update(|app| {
        let mut host = GpuiMainWindowShellHost::new(app, owner.clone());
        [
            host.construct_hidden(first)
                .unwrap_or_else(|_| panic!("first shell")),
            host.construct_hidden(second)
                .unwrap_or_else(|_| panic!("second shell")),
        ]
    });
    let ids = shells.each_ref().map(|shell| {
        shell
            .window()
            .read_with(cx, |root, app| {
                let controller = root.controller().unwrap();
                assert!(Arc::ptr_eq(controller.appearance(), &fixture.appearance));
                let mount = controller.composer_mount().unwrap();
                let editor = mount.read(app).contribution().unwrap();
                (
                    controller.window_id(),
                    mount.entity_id(),
                    editor.entity_id(),
                    editor.read(app).gpui_input().entity_id(),
                )
            })
            .unwrap()
    });
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[0].1, ids[1].1);
    assert_ne!(ids[0].2, ids[1].2);
    assert_ne!(ids[0].3, ids[1].3);
    for shell in &mut shells {
        assert!(!cx.window_visibility(shell.window().into()).is_visible);
        draw(shell.window(), cx);
        cx.update(|app| shell.publish(app)).unwrap();
    }
    let before = shells.each_ref().map(|shell| editor_snapshot(shell, cx));
    let mut coordinator = fixture.coordinator.take().unwrap();
    coordinator = support::join(
        support::worker(move || {
            preview(&mut coordinator, 2).unwrap();
            coordinator
        }),
        cx,
    );
    let current = coordinator.current();
    for (i, shell) in shells.iter().enumerate() {
        draw(shell.window(), cx);
        shell
            .window()
            .read_with(cx, |root, _| {
                assert!(Arc::ptr_eq(
                    root.controller().unwrap().appearance(),
                    &current
                ))
            })
            .unwrap();
        assert_eq!(editor_snapshot(shell, cx), before[i]);
    }
    let released = shells[1]
        .window()
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
        })
        .unwrap();
    for _ in 0..32 {
        let ready = shells[1]
            .window()
            .update(cx, |_, window, app| {
                released.update(app, |editor, cx| {
                    editor.begin_widget_release_fence(window, cx).unwrap()
                })
            })
            .unwrap();
        if ready {
            break;
        }
        draw(shells[1].window(), cx);
    }
    let _release = shells[1]
        .window()
        .update(cx, |_, window, app| {
            released.update(app, |editor, cx| editor.release_widget(window, cx))
        })
        .unwrap();
    let (coordinator, result) = support::join(
        support::worker(move || {
            let result = preview(&mut coordinator, 3);
            (coordinator, result)
        }),
        cx,
    );
    assert!(result.is_err());
    assert!(Arc::ptr_eq(&coordinator.current(), &current));
    for shell in &shells {
        shell
            .window()
            .read_with(cx, |root, _| {
                assert!(Arc::ptr_eq(
                    root.controller().unwrap().appearance(),
                    &current
                ))
            })
            .unwrap();
    }
    assert_eq!(
        owner.read_with(cx, |owner, _| owner.target().snapshot().count),
        2
    );
}

#[gpui::test]
fn native_minimum_has_usable_editor_and_absent_optional_mounts_consume_no_rows(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(91);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    let minimum = shell
        .window()
        .read_with(cx, |root, _| root.controller().unwrap().minimum_size())
        .unwrap();
    let client_minimum = shell
        .window()
        .update(cx, |_, window, _| {
            let scale = f64::from(window.scale_factor());
            gpui::size(
                gpui::px(((f64::from(minimum.width) * scale).ceil() / scale) as f32),
                gpui::px(((f64::from(minimum.height) * scale).ceil() / scale) as f32),
            )
        })
        .unwrap();
    assert!(client_minimum.width >= minimum.width && client_minimum.height >= minimum.height);
    cx.simulate_window_resize(shell.window().into(), client_minimum);
    draw(shell.window(), cx);
    let viewport = shell
        .window()
        .update(cx, |_, window, _| window.viewport_size())
        .unwrap();
    assert_eq!(viewport, client_minimum);
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    let panel = visual.debug_bounds("main-window-user-input-panel").unwrap();
    let editor = visual.debug_bounds("conversation-composer-root").unwrap();
    let transcript = visual
        .debug_bounds("main-window-transcript-region")
        .unwrap();
    let toolbar = visual.debug_bounds("main-window-toolbar").unwrap();
    let status = visual.debug_bounds("main-window-status-line").unwrap();
    assert!(editor.size.width >= gpui::px(30.));
    assert!(editor.size.height >= gpui::px(16.));
    assert_eq!(panel.top(), transcript.bottom());
    assert_eq!(toolbar.bottom(), transcript.top());
    assert_eq!(panel.bottom(), status.top());
    assert_eq!(status.bottom(), viewport.height);
    assert!(panel.size.height <= viewport.height * 0.5);
    for selector in [
        "main-window-new-thread-split-button",
        "main-window-thread-back",
        "main-window-thread-forward",
        "main-window-thread-selector",
        "main-window-running-threads",
        "main-window-exit",
        "main-window-status-model",
        "main-window-status-context:unknown",
        "main-window-status-turn",
    ] {
        let bounds = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing {selector}"));
        assert!(
            bounds.left() >= gpui::px(0.) && bounds.right() <= viewport.width,
            "{selector} escapes {viewport:?}: {bounds:?}"
        );
        assert!(
            bounds.top() >= gpui::px(0.) && bounds.bottom() <= viewport.height,
            "{selector} escapes {viewport:?}: {bounds:?}"
        );
        assert!(bounds.size.width > gpui::px(0.) && bounds.size.height > gpui::px(0.));
    }
    cx.update(|app| shell.publish(app)).unwrap();
    assert!(cx.window_visibility(shell.window().into()).is_visible);
}

#[gpui::test]
fn production_host_mount_failure_after_native_construction_returns_unpublished_custody(
    cx: &mut gpui::TestAppContext,
) {
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(101);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let result = cx.update(|app| {
        let mut host = GpuiMainWindowShellHost::new(app, owner.clone());
        host.test_reject_mount_after_native();
        host.construct_hidden(prepared)
    });
    let Err(MainWindowShellHostFailure::Construction { unpublished, .. }) = result else {
        panic!("post-native mount must fail")
    };
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(
        owner.read_with(cx, |owner, _| owner.target().snapshot().count),
        0
    );
    let result = support::join(
        support::worker(move || {
            let beryl_app::main_window::MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } = unpublished.prepare_abandonment(&fixture.service, CommandCancellation::new()) else { panic!("exact cleanup") };
            assert!(matches!(
                abandonment.abandon(&fixture.service, CommandCancellation::new()),
                beryl_app::main_window::MainWindowShellAbandonmentOutcome::Committed { .. }
            ));
            fixture.process.main_window_occupancy()
        }),
        cx,
    );
    assert_eq!(result, 0);
}

#[gpui::test]
fn stale_selection_after_hidden_construction_refuses_publication_and_retains_custody(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(111);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("shell"))
    });
    assert!(cx.update(|app| shell.publish(app)).is_err());
    assert!(!cx.window_visibility(shell.window().into()).is_visible);
    draw(shell.window(), cx);
    let editor = shell
        .window()
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
        })
        .unwrap();
    let prior = editor.read_with(cx, |editor, _| editor.selection_identity());
    shell
        .window()
        .update(cx, |_, window, app| {
            editor
                .read(app)
                .gpui_input()
                .update(app, |input, _| input.focus(window))
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    visual.simulate_input("x");
    for _ in 0..32 {
        draw(shell.window(), cx);
        if editor.read_with(cx, |editor, _| editor.selection_identity()) != prior {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_ne!(
        editor.read_with(cx, |editor, _| editor.selection_identity()),
        prior
    );
    assert!(cx.update(|app| shell.publish(app)).is_err());
    assert_eq!(
        cx.window_visibility(shell.window().into())
            .visibility_change_count,
        0
    );
    let unpublished = cx
        .update(|app| shell.close_before_publication(app))
        .unwrap_or_else(|_| panic!("unpublished close"));
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert_eq!(unpublished.window_id(), prior.window_id());
}
