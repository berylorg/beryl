use super::*;
use crate::thread_root_picker::{PickerRowKey, ThreadRootPicker};

#[path = "switcher_entry/elsewhere.rs"]
mod elsewhere;

pub(super) async fn open(
    window: WindowHandle<MainWindowShellRoot>,
    target: SyndicThreadId,
    cx: &mut gpui::AsyncApp,
) -> (gpui::Entity<ThreadRootPicker>, PickerRowKey) {
    window
        .update(cx, |root, window, app| {
            root.open_thread_switcher(window, app)
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pair) = cx
            .update(|app| {
                let root = window.read(app).unwrap();
                Some((
                    root.test_thread_switcher_picker()?,
                    root.test_thread_switcher_thread_key(target)?,
                ))
            })
            .unwrap()
        {
            return pair;
        }
        assert!(
            Instant::now() < deadline,
            "actual target picker row was not ready"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

#[test]
fn native_switcher_dirty_activation_recovers_exact_target_and_coherent_title_once() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, window_id, claim) = scenario::recover_with_entry(
                    owner,
                    faults,
                    Cut::ClaimCommitted,
                    control::Control::Normal,
                    SaveExpectation::Dirty,
                    scenario::Entry::Switcher,
                    cx,
                )
                .await;
                cx.update(|app| {
                    let root = window.read(app).unwrap();
                    assert!(root.test_thread_switcher_picker().is_none());
                    let (selected, title) = root.coherent_selected_thread_title(app).unwrap();
                    assert_eq!(selected, claim);
                    assert_eq!(
                        title.source(),
                        beryl_state::CatalogTitleSource::HistoryDerived
                    );
                })
                .unwrap();
                *qualified_selection.borrow_mut() = Some((window_id, claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}

#[test]
fn native_switcher_noncommit_then_disposal_recovery_releases_original_adopted_pages() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, window_id, prior) = scenario::recover_with_entry(
                    owner.clone(),
                    faults.clone(),
                    Cut::SaveNoncommit,
                    control::Control::Normal,
                    SaveExpectation::DirtyPageSetup,
                    scenario::Entry::Switcher,
                    cx,
                )
                .await;
                assert!(
                    cx.update(|app| window
                        .read(app)
                        .unwrap()
                        .test_thread_creation_composer_adopted_custody_items(app))
                        .unwrap()
                        > 0
                );
                let (same_window, same_id, selected) = scenario::recover_with_entry(
                    owner,
                    faults,
                    Cut::DisposalCommitted,
                    control::Control::Normal,
                    SaveExpectation::SavedNoop,
                    scenario::Entry::Switcher,
                    cx,
                )
                .await;
                assert_eq!(same_window, window);
                assert_eq!(same_id, window_id);
                assert_ne!(selected.thread_id(), prior.thread_id());
                *qualified_selection.borrow_mut() = Some((same_id, selected.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
