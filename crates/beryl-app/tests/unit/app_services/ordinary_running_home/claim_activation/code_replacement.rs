use super::*;
use crate::syndic_transcript::ResourceViewportDemandChanged;
pub(super) use native_code::prepare_history;

#[test]
fn native_committed_target_replacement_loads_code_and_routes_fresh_inner_ranges() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home_with_code,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let prior = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let (window, window_id, claim) = scenario::recover_with_entry(
                    owner,
                    faults,
                    Cut::ClaimCommitted,
                    control::Control::Normal,
                    SaveExpectation::Dirty,
                    scenario::Entry::PreparedWithCode,
                    cx,
                )
                .await;
                *qualified_selection.borrow_mut() = Some((window_id, claim.thread_id()));
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                assert_ne!(prior.entity_id(), panel.entity_id());
                native_code::qualify(window, &panel, cx).await;
                let notifications = Rc::new(std::cell::Cell::new(0));
                let observed = notifications.clone();
                let observer = cx
                    .update(|app| {
                        let root = window.root(app).unwrap();
                        app.observe(&root, move |_, _| observed.set(observed.get() + 1))
                    })
                    .unwrap();
                cx.update(|app| {
                    prior.update(app, |_, cx| cx.emit(ResourceViewportDemandChanged));
                })
                .unwrap();
                assert_eq!(notifications.get(), 0);
                cx.update(|app| {
                    panel.update(app, |_, cx| cx.emit(ResourceViewportDemandChanged));
                })
                .unwrap();
                assert_eq!(notifications.get(), 1);
                drop(observer);
                assert!(
                    prior
                        .read_with(cx, |panel, _| panel
                            .diagnostic_snapshot()
                            .code_panels
                            .is_empty())
                        .unwrap()
                );
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
