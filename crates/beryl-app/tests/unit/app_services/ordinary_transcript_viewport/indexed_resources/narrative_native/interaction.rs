use super::super::code_long_line::{wait_until_with_visibility, with_native};
use super::super::*;
use crate::syndic_transcript::{ResidentPresentationRecordId, SyndicTranscriptPanel};
use gpui::{AsyncApp, Entity, Pixels, PlatformInput, Point, WindowHandle};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(in super::super) fn require_fixture_ledger() {
    let ledger = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
        .expect("native narrative fixtures require their retained resource ledger");
    assert!(std::path::Path::new(&ledger).is_absolute());
}

pub(in super::super) fn key(native: &mut gpui::Window, app: &mut gpui::App, value: &str) {
    native.dispatch_input_for_test(
        PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(value).unwrap(),
            is_held: false,
        }),
        app,
    );
    native.draw_and_present_for_test(app);
}

pub(in super::super) fn pointer(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    input: PlatformInput,
) {
    native.dispatch_input_for_test(input, app);
    native.draw_and_present_for_test(app);
}

pub(super) fn press(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: Point<Pixels>,
    shift: bool,
) {
    pointer(
        native,
        app,
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            click_count: 1,
            ..Default::default()
        }),
    );
}

pub(super) fn release(native: &mut gpui::Window, app: &mut gpui::App, position: Point<Pixels>) {
    pointer(
        native,
        app,
        PlatformInput::MouseUp(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }),
    );
}

pub(in super::super) fn drag(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    start: Point<Pixels>,
    end: Point<Pixels>,
) {
    press(native, app, start, false);
    pointer(
        native,
        app,
        PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position: end,
            pressed_button: Some(gpui::MouseButton::Left),
            modifiers: Default::default(),
        }),
    );
    release(native, app, end);
}

pub(in super::super) async fn ready(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
    needles: &[&str],
) {
    ready_with_visibility(window, panel, cx, needles, true).await;
}

pub(super) async fn ready_with_visibility(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
    needles: &[&str],
    minimize: bool,
) {
    with_native(window, cx, |native, app| {
        native.draw_and_present_for_test(app);
        if minimize {
            native.minimize_window();
        }
    })
    .unwrap();
    wait_until_with_visibility(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, app| {
                let panel = panel.read(app);
                let current_geometry = panel.narrative_geometry_current_for_test(native);
                let facts = panel.narrative_layout_facts_for_test();
                current_geometry
                    && panel.measured_frame_for_test().is_some()
                    && needles.iter().all(|needle| {
                        facts.iter().any(|(id, source, display, fragments)| {
                            source.contains(needle)
                                && *fragments > 0
                                && *fragments <= 32
                                && panel.narrative_position_for_test(id, 0).is_some()
                                && panel
                                    .narrative_position_for_test(
                                        id,
                                        display.trim_end_matches('\n').len(),
                                    )
                                    .is_some()
                        })
                    })
            })
            .unwrap()
        },
        "saved narrative must publish measured, painted mapped geometry",
        minimize,
    )
    .await;
}

pub(in super::super) fn record(
    panel: &SyndicTranscriptPanel,
    needle: &str,
) -> (ResidentPresentationRecordId, String, String) {
    panel
        .narrative_text_for_test()
        .into_iter()
        .find(|(_, source, _)| source.contains(needle))
        .expect("expected genuine saved narrative record")
}

pub(in super::super) fn point_at(
    panel: &SyndicTranscriptPanel,
    id: &ResidentPresentationRecordId,
    offset: usize,
) -> Point<Pixels> {
    panel
        .narrative_position_for_test(id, offset)
        .expect("endpoint must belong to current painted narrative geometry")
}

pub(in super::super) fn writer(
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) -> (Rc<RefCell<Vec<String>>>, Rc<Cell<bool>>) {
    let captured = Rc::new(RefCell::new(Vec::new()));
    let refuse = Rc::new(Cell::new(false));
    let sink = captured.clone();
    let fail = refuse.clone();
    panel
        .update(cx, |panel, _| {
            panel.set_keyboard_copy_writer_for_test(move |item, _, _| {
                if fail.get() {
                    return Err(());
                }
                assert!(sink.borrow().len() < 32);
                sink.borrow_mut().push(item.text().unwrap());
                Ok(())
            })
        })
        .unwrap();
    (captured, refuse)
}

pub(in super::super) fn copy_both(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    captured: &Rc<RefCell<Vec<String>>>,
    markdown: &str,
    stripped: &str,
) {
    let before = captured.borrow().len();
    key(native, app, "ctrl-c");
    assert_eq!(captured.borrow().len(), before + 1);
    assert_eq!(captured.borrow()[before], markdown);
    native.dispatch_input_for_test(
        PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-c").unwrap(),
            is_held: true,
        }),
        app,
    );
    assert_eq!(captured.borrow().len(), before + 1);
    key(native, app, "ctrl-c");
    assert_eq!(captured.borrow().len(), before + 2);
    assert_eq!(captured.borrow()[before + 1], stripped);
}
