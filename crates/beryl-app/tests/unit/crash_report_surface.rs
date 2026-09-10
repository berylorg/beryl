use super::*;

#[derive(Default)]
pub(super) struct Commands {
    pub copies: Vec<String>,
    pub copy_success: bool,
    pub exits: usize,
}

fn mount(cx: &mut gpui::TestAppContext, report: String) -> gpui::WindowHandle<CrashReport> {
    let window = cx.update(|app| {
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(660.), px(460.)),
                    app,
                ))),
                ..Default::default()
            },
            |window, app| app.new(|cx| CrashReport::new(report, window, cx)),
        )
        .unwrap()
    });
    cx.run_until_parked();
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| window.draw(app).clear())
            .unwrap();
    });
    window
}

fn key(visual: &mut gpui::VisualTestContext, value: &str) {
    let keystroke = gpui::Keystroke::parse(value).unwrap();
    visual.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
    });
    visual.simulate_event(gpui::KeyUpEvent { keystroke });
}

#[gpui::test]
fn keyboard_has_two_focus_targets_and_never_exits_from_initial_copy_focus(
    cx: &mut gpui::TestAppContext,
) {
    let window = mount(cx, "panic report".into());
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.debug_bounds("crash-report-copy").unwrap();
    window
        .update(cx, |view, window, _| {
            assert!(view.copy_focus.is_focused(window))
        })
        .unwrap();
    key(&mut visual, "enter");
    key(&mut visual, "escape");
    key(&mut visual, "ctrl-r");
    window
        .update(cx, |view, _, _| {
            assert_eq!(view.commands.copies, ["panic report"]);
            assert_eq!(view.commands.exits, 0);
        })
        .unwrap();
    key(&mut visual, "tab");
    window
        .update(cx, |view, window, _| {
            assert!(view.exit_focus.is_focused(window))
        })
        .unwrap();
    key(&mut visual, "space");
    window
        .update(cx, |view, _, _| assert_eq!(view.commands.exits, 1))
        .unwrap();
    key(&mut visual, "shift-tab");
    window
        .update(cx, |view, window, _| {
            assert!(view.copy_focus.is_focused(window))
        })
        .unwrap();
}

#[gpui::test]
fn copy_failure_preserves_commands_and_retry_exports_the_complete_report(
    cx: &mut gpui::TestAppContext,
) {
    let report = "<button>Retry</button>\n".repeat(50);
    let window = mount(cx, report.clone());
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let copy = visual.debug_bounds("crash-report-copy").unwrap();
    visual.simulate_click(copy.center(), gpui::Modifiers::none());
    window
        .update(cx, |view, _, _| {
            assert!(view.feedback.starts_with("Could not copy"));
            assert_eq!(view.commands.copies, [report.clone()]);
            assert_eq!(view.commands.exits, 0);
            view.commands.copy_success = true;
        })
        .unwrap();
    visual.simulate_click(copy.center(), gpui::Modifiers::none());
    window
        .update(cx, |view, _, _| {
            assert_eq!(view.feedback, "Report copied.");
            assert_eq!(view.commands.copies, [report.clone(), report]);
        })
        .unwrap();
    let exit = visual.debug_bounds("crash-report-exit").unwrap();
    visual.simulate_click(exit.center(), gpui::Modifiers::none());
    window
        .update(cx, |view, _, _| assert_eq!(view.commands.exits, 1))
        .unwrap();
}

#[gpui::test]
fn maximum_preview_keeps_both_commands_and_feedback_inside_the_window(
    cx: &mut gpui::TestAppContext,
) {
    let window = mount(cx, format!("{}\n", "🙂".repeat(100)).repeat(10));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    let root = visual.debug_bounds("crash-report").unwrap();
    for selector in [
        "crash-report-copy",
        "crash-report-exit",
        "crash-report-copy-result",
        "crash-report-preview",
    ] {
        let bounds = visual.debug_bounds(selector).unwrap();
        assert!(root.contains(&bounds.origin));
        assert!(bounds.bottom() <= root.bottom());
        assert!(bounds.right() <= root.right());
    }
    assert!(visual.debug_bounds("Retry").is_none());
    let preview = visual.debug_bounds("crash-report-preview").unwrap();
    let omission = visual
        .debug_bounds("crash-report-preview-omission")
        .unwrap();
    assert!(omission.bottom() <= preview.bottom());
}

#[test]
fn preview_limits_lines_and_marks_omission_without_changing_the_report() {
    let report = "diagnostic\n".repeat(20);
    let (lines, omitted) = preview(&report);
    assert_eq!(lines.len(), 9);
    assert!(omitted);
    assert_eq!(report.lines().count(), 20);
}

#[test]
fn preview_bounds_unicode_lines_and_renders_markup_as_text() {
    let report = format!("<button>Exit</button>\n{}", "🙂".repeat(120));
    let (lines, omitted) = preview(&report);
    assert_eq!(lines[0].as_ref(), "<button>Exit</button>");
    assert_eq!(lines[1].chars().count(), 101);
    assert!(lines[1].ends_with('…'));
    assert!(omitted);
}
