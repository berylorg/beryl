use super::*;
use gpui::{EntityInputHandler, WindowHandle};

pub(super) fn verify(window: WindowHandle<MainWindowShellRoot>, cx: &mut TestAppContext) {
    let panel = resource_wakeups::capture(window, cx);
    let input = window
        .read_with(cx, |root, app| {
            let mount = root.controller().unwrap().composer_mount().unwrap();
            mount
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .gpui_input()
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            input.read_with(cx, |input, _| {
                input.is_semantically_quiescent() && input.is_surface_current_and_interactive()
            })
        },
        "recovered editor did not become interactive",
    );
    let text = format!("```rust\n{}\n```", "let payload = 1;\n".repeat(2000));
    cx.update(|app| {
        app.update_window(window.into(), |_, window, app| {
            input.update(app, |input, cx| {
                input.replace_text_in_range(None, &text, window, cx)
            });
        })
        .unwrap();
    });
    wait(
        cx,
        |cx| {
            input.read_with(cx, |input, _| {
                input.is_semantically_quiescent()
                    && input.is_surface_current_and_interactive()
                    && input.surface().is_some_and(|surface| {
                        surface.source_selection().head.byte_offset.get() == text.len() as u64
                    })
            })
        },
        "recovered Code draft edit did not settle",
    );
    for _ in 0..10 {
        cx.run_until_parked();
        cx.update(|app| {
            app.update_window(window.into(), |_, window, app| window.draw(app).clear())
                .unwrap();
        });
        panel.read_with(cx, |panel, _| {
            let snapshot = panel.snapshot();
            assert!(snapshot.records.is_empty());
            assert!(snapshot.resources.metadata.is_empty());
            assert!(panel.code_viewport_for_test().is_none());
            assert!(panel.diagnostic_snapshot().code_panels.is_empty());
            assert!(panel.resource_demand().is_none());
        });
    }
}
