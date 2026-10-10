use super::*;
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{VK_RETURN, VK_SPACE},
    WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP},
};

#[path = "lineage_entry/elsewhere.rs"]
mod elsewhere;
#[path = "lineage_entry/publication.rs"]
mod publication;

#[derive(Clone, Copy)]
pub(super) enum Input {
    Pointer,
    Enter,
    Space,
}

pub(super) async fn prepare(window: WindowHandle<MainWindowShellRoot>, cx: &mut gpui::AsyncApp) {
    let child = lineage_fixture::child();
    let deadline = Instant::now() + Duration::from_secs(20);
    let selected = loop {
        let selected = window
            .read_with(cx, |root, app| {
                (!root.test_running_thread_activation_pending())
                    .then(|| root.coherent_selected_thread_title(app))
                    .flatten()
                    .map(|(claim, _)| claim.thread_id())
            })
            .unwrap();
        if let Some(selected) = selected {
            break selected;
        }
        assert!(
            Instant::now() < deadline,
            "original selected thread title did not become coherent"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    };
    if selected != child {
        let (picker, key) = switcher_entry::open(window, child, cx).await;
        window
            .update(cx, |_, _, app| {
                picker.update(app, |picker, cx| picker.activate(&key, cx))
            })
            .unwrap();
    }
    ready(window, SyndicThreadId::from_bytes([242; 16]), cx).await;
    window
        .read_with(cx, |root, app| {
            assert_eq!(
                root.coherent_selected_thread_title(app)
                    .unwrap()
                    .0
                    .thread_id(),
                child
            );
            assert_eq!(root.test_thread_navigation_history().last(), Some(&child));
            assert!(!root.test_running_thread_activation_pending());
        })
        .unwrap();
}

pub(super) async fn ready(
    window: WindowHandle<MainWindowShellRoot>,
    parent: SyndicThreadId,
    cx: &mut gpui::AsyncApp,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (available, readiness) = window
            .update(cx, |root, window, app| {
                let available = root.test_thread_lineage_view().is_some_and(|widget| {
                    widget
                        .read(app)
                        .test_parent_input(parent, window)
                        .is_some_and(|(_, _, available)| available)
                });
                (available, root.test_lineage_readiness(app))
            })
            .unwrap();
        if available {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "exact lineage parent did not become available: {readiness}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

pub(super) async fn focus(
    window: WindowHandle<MainWindowShellRoot>,
    parent: SyndicThreadId,
    cx: &mut gpui::AsyncApp,
) {
    window
        .update(cx, |root, window, app| {
            let widget = root.test_thread_lineage_view().unwrap();
            widget.update(app, |widget, cx| {
                widget.test_focus_parent(parent, window, cx)
            });
        })
        .unwrap();
}

pub(super) fn post_input(
    root: &MainWindowShellRoot,
    parent: SyndicThreadId,
    input: Input,
    handle: windows::Win32::Foundation::HWND,
    window: &mut gpui::Window,
    app: &mut gpui::Context<MainWindowShellRoot>,
) {
    let widget = root.test_thread_lineage_view().unwrap();
    assert!(
        widget.read(app).test_logical_focus(parent, window),
        "original parent input lacks actual lineage focus: {}",
        root.test_lineage_readiness(app)
    );
    assert!(
        widget
            .read(app)
            .test_parent_input(parent, window)
            .is_some_and(|(_, _, available)| available),
        "original parent input is unavailable: {}",
        root.test_lineage_readiness(app)
    );
    match input {
        Input::Pointer => {
            let widget = root.test_thread_lineage_view().unwrap();
            let (point, _, available) = widget.read(app).test_parent_input(parent, window).unwrap();
            assert!(available);
            let x = (f32::from(point.x) * window.scale_factor()).round() as u32;
            let y = (f32::from(point.y) * window.scale_factor()).round() as u32;
            let client = LPARAM(((y << 16) | (x & 0xffff)) as isize);
            unsafe {
                PostMessageW(Some(handle), WM_LBUTTONDOWN, WPARAM(1), client).unwrap();
            }
            window
                .spawn(app, async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                    unsafe {
                        PostMessageW(Some(handle), WM_LBUTTONUP, WPARAM(0), client).unwrap();
                    }
                })
                .detach();
        }
        Input::Enter | Input::Space => {
            let key = if matches!(input, Input::Enter) {
                VK_RETURN.0
            } else {
                VK_SPACE.0
            };
            unsafe {
                PostMessageW(Some(handle), WM_KEYDOWN, WPARAM(key as usize), LPARAM(1)).unwrap();
                PostMessageW(
                    Some(handle),
                    WM_KEYUP,
                    WPARAM(key as usize),
                    LPARAM(0xc0000001u32 as isize),
                )
                .unwrap();
            }
        }
    }
}

pub(super) async fn assert_history_settled_once(
    window: WindowHandle<MainWindowShellRoot>,
    child: SyndicThreadId,
    parent: SyndicThreadId,
    committed: bool,
    cx: &mut gpui::AsyncApp,
) {
    window
        .update(cx, |root, _, _| {
            let history = root.test_thread_navigation_history();
            assert_eq!(history.iter().filter(|id| **id == child).count(), 1);
            assert_eq!(
                history.iter().filter(|id| **id == parent).count(),
                usize::from(committed)
            );
            assert_eq!(
                history.last(),
                Some(if committed { &parent } else { &child })
            );
            assert_eq!(history.len(), if committed { 3 } else { 2 });
            if committed {
                assert!(root.test_thread_lineage_view().is_none());
            }
        })
        .unwrap();
}

#[test]
fn native_lineage_pointer_dirty_claim_recovers_original_parent_and_history_once() {
    run(Input::Pointer, Cut::ClaimCommitted, SaveExpectation::Dirty);
}

#[test]
fn native_lineage_enter_save_noncommit_preserves_child_editor_and_lineage_focus() {
    run(Input::Enter, Cut::SaveNoncommit, SaveExpectation::Dirty);
}

#[test]
fn native_lineage_space_disposal_recovery_keeps_original_page_receipt_and_parent_history_once() {
    run(
        Input::Space,
        Cut::DisposalCommitted,
        SaveExpectation::SavedNoop,
    );
}

fn run(input: Input, cut: Cut, save: SaveExpectation) {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        lineage_fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                if matches!(input, Input::Space) {
                    let original_window = windows(&owner)[0];
                    prepare(original_window, cx).await;
                    let (original_claim, original_history) = original_window
                        .read_with(cx, |root, app| {
                            (
                                root.coherent_selected_thread_title(app).unwrap().0,
                                root.test_thread_navigation_history().to_vec(),
                            )
                        })
                        .unwrap();
                    let (same_window, _, same_claim) = scenario::recover_with_entry(
                        owner.clone(),
                        faults.clone(),
                        Cut::SaveNoncommit,
                        control::Control::Normal,
                        SaveExpectation::DirtyPageSetup,
                        scenario::Entry::PreparedWithParentFocus,
                        cx,
                    )
                    .await;
                    assert!(same_window == original_window);
                    assert_eq!(same_claim, original_claim);
                    assert_eq!(same_claim.thread_id(), lineage_fixture::child());
                    same_window
                        .read_with(cx, |root, app| {
                            assert!(
                                root.test_thread_creation_composer_adopted_custody_items(app) > 0,
                                "original child recovery did not adopt genuine Page custody"
                            );
                            assert_eq!(
                                root.test_thread_navigation_history(),
                                original_history.as_slice()
                            );
                        })
                        .unwrap();
                }
                let (window, id, claim) = scenario::recover_with_entry(
                    owner,
                    faults,
                    cut,
                    control::Control::Normal,
                    save,
                    scenario::Entry::Lineage(input),
                    cx,
                )
                .await;
                *qualified.borrow_mut() = Some((id, claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
