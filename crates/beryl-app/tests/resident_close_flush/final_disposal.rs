use beryl_app::main_window::MainWindowConversationComposerCloseAdvance as Advance;

use super::support::{self, drive};

#[gpui::test]
fn cancelled_final_disposal_preserves_disabled_editor_and_allows_a_new_explicit_close(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "cancelled-final-disposal", 201);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let resident = fixture.service.selected_identity().unwrap();
    let first = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    super::mounted::ready(&fixture, first.ticket, cx);
    cx.update(|_, app| input.update(app, |input, cx| input.set_enabled(false, cx)));
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            mount
                .authorize_window_close_disposal(first.ticket, window, cx)
                .unwrap();
            mount.test_cancel_next_window_close_disposal();
        })
    });
    let mut failed = false;
    for _ in 0..512 {
        drive(cx, 1);
        let state = cx
            .update(|window, app| {
                fixture.mount.update(app, |mount, cx| {
                    mount.advance_window_close(first.ticket, window, cx)
                })
            })
            .unwrap();
        match state {
            Advance::Unsatisfied(_) => {
                failed = true;
                break;
            }
            Advance::Disposed | Advance::Stale => panic!("cancelled disposal settled as {state:?}"),
            _ => {}
        }
    }
    assert!(failed, "cancelled final disposal did not settle");
    assert_eq!(fixture.service.selected_identity(), Some(resident));
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        composer.entity_id()
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    assert!(input.read_with(cx, |input, _| input.surface().is_some()));
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                first.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    let second = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    assert_ne!(first.ticket, second.ticket);
    super::mounted::ready(&fixture, second.ticket, cx);
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.release_window_close(
                second.ticket,
                window,
                cx
            )))
        .unwrap()
    );
    assert_eq!(fixture.service.selected_identity(), Some(resident));
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    drop((composer, input));
    support::finish(fixture, cx);
}
