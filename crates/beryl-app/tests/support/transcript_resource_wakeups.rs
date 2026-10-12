use crate::main_window::MainWindowShellRoot;
use crate::syndic_transcript::{ResourceViewportDemandChanged, SyndicTranscriptPanel};
use gpui::{AppContext, Entity, TestAppContext, WindowHandle};
use std::{cell::Cell, rc::Rc};

pub(super) fn capture(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> Entity<SyndicTranscriptPanel> {
    window
        .read_with(cx, |root, _| root.test_transcript_panel())
        .unwrap()
}

pub(super) fn verify_replacement(
    window: WindowHandle<MainWindowShellRoot>,
    prior: Entity<SyndicTranscriptPanel>,
    cx: &mut TestAppContext,
) {
    verify_binding(window, prior, true, cx);
}

pub(super) fn verify_retained(
    window: WindowHandle<MainWindowShellRoot>,
    prior: Entity<SyndicTranscriptPanel>,
    cx: &mut TestAppContext,
) {
    verify_binding(window, prior, false, cx);
}

fn verify_binding(
    window: WindowHandle<MainWindowShellRoot>,
    prior: Entity<SyndicTranscriptPanel>,
    replaced: bool,
    cx: &mut TestAppContext,
) {
    cx.run_until_parked();
    let current = capture(window, cx);
    assert_eq!(prior.entity_id() != current.entity_id(), replaced);
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let observer = cx.update(|app| {
        let root = window.root(app).unwrap();
        app.observe(&root, move |_, _| observed.set(observed.get() + 1))
    });
    if replaced {
        prior.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
        cx.run_until_parked();
        assert_eq!(
            notifications.get(),
            0,
            "retired transcript must not wake the shell"
        );
    }
    current.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
    cx.run_until_parked();
    assert_eq!(
        notifications.get(),
        1,
        "recovered transcript must wake the shell once"
    );
    drop((observer, prior, current));
}
