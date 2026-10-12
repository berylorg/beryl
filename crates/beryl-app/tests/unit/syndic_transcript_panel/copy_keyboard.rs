use super::super::copy_keyboard::{CopyGesture, CopyMethod, CopyTarget};
use super::*;
use std::time::{Duration, Instant};

fn target(id: u64) -> CopyTarget {
    CopyTarget::Code {
        id,
        revision: ProviderRevision(1),
        root: [1; 32],
        range: 8..12,
    }
}

#[test]
fn distinct_copy_pair_is_immediate_ignores_repeat_and_starts_a_new_pair() {
    let mut gesture = CopyGesture::default();
    let now = Instant::now();
    assert_eq!(
        gesture.press(target(1), false, now),
        Some(CopyMethod::Markdown)
    );
    assert_eq!(
        gesture.press(target(1), true, now + Duration::from_millis(50)),
        None
    );
    assert_eq!(
        gesture.press(target(1), false, now + Duration::from_millis(100)),
        Some(CopyMethod::Stripped)
    );
    assert_eq!(
        gesture.press(target(1), false, now + Duration::from_millis(150)),
        Some(CopyMethod::Markdown)
    );
}

#[test]
fn copy_pair_requires_current_target_and_interval_and_resets_after_focus_or_selection_change() {
    let mut gesture = CopyGesture::default();
    let now = Instant::now();
    assert_eq!(
        gesture.press(target(1), false, now),
        Some(CopyMethod::Markdown)
    );
    assert_eq!(
        gesture.press(target(2), false, now),
        Some(CopyMethod::Markdown)
    );
    assert_eq!(
        gesture.press(target(2), false, now + Duration::from_millis(601)),
        Some(CopyMethod::Markdown)
    );
    gesture.reset();
    assert_eq!(
        gesture.press(target(2), false, now + Duration::from_millis(602)),
        Some(CopyMethod::Markdown)
    );
    let mut changed = target(2);
    if let CopyTarget::Code { range, .. } = &mut changed {
        *range = 8..13;
    }
    assert_eq!(
        gesture.press(changed, false, now + Duration::from_millis(603)),
        Some(CopyMethod::Markdown)
    );
}

#[gpui::test]
fn mounted_keyboard_copy_uses_admitted_representations_and_preserves_selection_on_refusal(
    cx: &mut TestAppContext,
) {
    let mut activation = seed(1);
    if let Some(TranscriptProviderResponseKind::ProjectionRecords(records)) =
        &mut activation.projection_records_response
    {
        records.records[0].payload = ProjectionPayload::Text {
            text: "**original**".into(),
            plain_text: Some("original".into()),
        };
        records.records[0].provenance.copy_source_range = Some(0..12);
        records.records[0].provenance.source_range = Some(0..12);
    }
    if let Some(TranscriptProviderResponseKind::ViewPage(page)) = &mut activation.view_page_response
    {
        page.records[0].provenance.copy_source_range = Some(0..12);
        page.records[0].provenance.source_range = Some(0..12);
    }
    let window = mounted_panel(cx, activation);
    draw_frames(cx, window, 4);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let copied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let writes = copied.clone();
    window
        .update(cx, |_, window, app| {
            panel.update(app, |panel, cx| {
                let record = panel.snapshot().records[0].id.clone();
                let outcome = panel.apply_realized_selection_for_record_ids(&[record]);
                assert!(
                    matches!(outcome, ResidentSelectionOutcome::Selected(_)),
                    "{outcome:?}"
                );
                panel.keyboard_copy.writer = Some(Box::new(move |item, _, _| {
                    writes.borrow_mut().push(item.text().unwrap());
                    Ok(())
                }));
                window.focus(&panel.focus_handle);
                cx.notify();
            });
        })
        .unwrap();
    draw_frames(cx, window, 2);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("ctrl-c");
    assert_eq!(&*copied.borrow(), &["**original**"]);
    visual.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-c").unwrap(),
        is_held: true,
    });
    assert_eq!(copied.borrow().len(), 1);
    visual.simulate_keystrokes("ctrl-c");
    assert_eq!(&*copied.borrow(), &["**original**", "original"]);
    panel.update(cx, |panel, _| {
        panel.keyboard_copy.writer = Some(Box::new(|_, _, _| Err(())));
    });
    visual.simulate_keystrokes("ctrl-c");
    panel.read_with(cx, |panel, _| {
        assert!(panel.host.resident_selection().is_some());
        assert!(panel.keyboard_copy.feedback.contains("refused"));
    });
}
