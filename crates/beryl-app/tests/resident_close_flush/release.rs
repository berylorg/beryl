use super::support::{self, Mounted, drive_until};

#[gpui::test]
fn busy_slot_release_finishes_in_background_and_stale_release_preserves_the_next_close(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "close-worker-release", 211);
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
    fixture.service.test_with_close_slot_locked(|| {
        assert!(
            cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
                mount.release_window_close(first.ticket, window, cx)
            }))
            .unwrap()
        );
        assert!(fixture.service.test_window_close_is_current(first.ticket));
        assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    });
    drive_until(cx, "background close release", |_| {
        !fixture.service.test_window_close_is_current(first.ticket)
    });
    assert_eq!(fixture.service.selected_identity(), Some(resident));
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
        !cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
            mount.release_window_close(first.ticket, window, cx)
        }))
        .unwrap()
    );
    assert!(fixture.service.test_window_close_is_current(second.ticket));
    assert_eq!(fixture.service.selected_identity(), Some(resident));
    assert!(
        cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
            mount.release_window_close(second.ticket, window, cx)
        }))
        .unwrap()
    );
    assert!(!fixture.service.test_window_close_is_current(second.ticket));
    assert!(!input.read_with(cx, |input, _| input.is_enabled()));
    drop((composer, input));
    support::finish(fixture, cx);
}

#[gpui::test]
fn dropping_a_ready_close_releases_its_reservation_without_disposing_the_saved_editor(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = support::mounted(cx, "close-unmounted-release", 221);
    let resident = fixture.service.selected_identity().unwrap();
    let candidate = resident.binding().candidate();
    let session = fixture
        .storage
        .draft_editor_candidate_session(
            &fixture.store,
            candidate.draft_id(),
            candidate.session_id(),
        )
        .unwrap();
    assert!(matches!(
        session,
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(_)
    ));
    let revision = fixture.store.home_revision().unwrap();
    let close = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    super::mounted::ready(&fixture, close.ticket, cx);
    let weak_mount = fixture.mount.downgrade();
    let weak_service = std::sync::Arc::downgrade(&fixture.service);
    let Mounted {
        root,
        mount,
        service,
        store,
        storage,
        assets,
        seals,
        directory,
    } = fixture;
    cx.update(|window, _| window.remove_window());
    drop((root, mount));
    cx.cx.update(|_| ());
    drive_detached_until(cx, "unmounted close release", || {
        weak_mount.upgrade().is_none() && !service.test_window_close_is_current(close.ticket)
    });
    assert_eq!(service.selected_identity(), Some(resident));
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, candidate.draft_id(), candidate.session_id())
            .unwrap(),
        session
    );
    assert_eq!(store.home_revision().unwrap(), revision);
    drop(service);
    drive_detached_until(cx, "unmounted close service release", || {
        weak_service.upgrade().is_none()
    });
    drop((store, storage, assets, seals));
    directory
        .close()
        .expect("unmounted close resources released");
}

fn drive_detached_until(
    cx: &mut gpui::VisualTestContext,
    stage: &str,
    mut ready: impl FnMut() -> bool,
) {
    for _ in 0..512 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(1));
        cx.run_until_parked();
        if ready() {
            return;
        }
    }
    panic!("{stage} did not settle within 512 steps");
}
