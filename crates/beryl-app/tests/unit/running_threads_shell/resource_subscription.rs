use super::*;
use crate::syndic_transcript::{ResourceViewportDemandChanged, SyndicTranscriptPanel};
use std::cell::Cell;
use std::rc::Rc;

#[gpui::test]
fn transcript_replacement_routes_resource_wakeups_only_from_the_current_live_panel(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 197);
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let notified = notifications.clone();
    let observer = cx.update(|app| {
        let root = mounted.window.root(app).unwrap();
        app.observe(&root, move |_, _| notified.set(notified.get() + 1))
    });
    let mut prior = mounted
        .window
        .read_with(cx, |root, _| root.running_threads.transcript.clone())
        .unwrap();
    for _ in 0..2 {
        let current = mounted
            .window
            .update(cx, |root, window, cx| {
                let panel = cx.new(SyndicTranscriptPanel::new);
                root.running_threads
                    .replace_transcript(panel.clone(), window, cx);
                panel
            })
            .unwrap();
        assert_ne!(prior.entity_id(), current.entity_id());
        cx.run_until_parked();
        notifications.set(0);
        prior.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
        cx.run_until_parked();
        assert_eq!(notifications.get(), 0);
        current.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
        cx.run_until_parked();
        assert_eq!(notifications.get(), 1);

        mounted
            .window
            .update(cx, |root, _, _| root.running_threads.views_retired = true)
            .unwrap();
        notifications.set(0);
        current.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
        cx.run_until_parked();
        assert_eq!(notifications.get(), 0);
        mounted
            .window
            .update(cx, |root, _, _| root.running_threads.views_retired = false)
            .unwrap();
        current.update(cx, |_, cx| cx.emit(ResourceViewportDemandChanged));
        cx.run_until_parked();
        assert_eq!(notifications.get(), 1);
        prior = current;
    }
    drop((observer, prior));
    support::finish(mounted, cx);
}
