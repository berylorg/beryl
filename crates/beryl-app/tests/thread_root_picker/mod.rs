mod command_restoration;
mod commands;
mod history;
mod runtime;
mod setup;

use super::*;
use gpui::AppContext;
use std::{cell::RefCell, rc::Rc};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, app| window.draw(app).clear());
    cx.run_until_parked();
}

fn mount(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<ThreadRootPicker>, &mut gpui::VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let owner_focus = cx.focus_handle();
        let mut picker = ThreadRootPicker::new(
            ThreadRootPickerConfig {
                title: "New thread".into(),
                helper: "Choose a root".into(),
                heading: "ROOTS FOR ALL RUNTIMES".into(),
                empty_text: "No roots".into(),
                search_placeholder: "Search roots".into(),
                owner_focus,
                appearance: None,
                style: ThreadRootPickerStyle::default(),
                scrollbar_style: gpui_scrollbar::ScrollbarStyle::default(),
            },
            PickerCollectionKey("roots-all".into()),
            1,
            100_000,
            window,
            cx,
        );
        picker.configure_selection(
            PickerSelectionMode::Confirmed {
                confirm: PickerCommandState::enabled("Confirm"),
            },
            cx,
        );
        picker.set_row_presentation(PickerRowPresentation::Root, cx);
        picker
    })
}

fn admit(picker: &gpui::Entity<ThreadRootPicker>, cx: &mut gpui::VisualTestContext) {
    draw(cx);
    let requests = picker.read_with(cx, |picker, _| picker.pending_requests().to_vec());
    cx.update(|window, app| {
        picker.update(app, |picker, cx| {
            for request in requests {
                picker.settle_page(page(request, 100_000), window, cx);
            }
        })
    });
    draw(cx);
}

fn collect(
    picker: &gpui::Entity<ThreadRootPicker>,
    cx: &mut gpui::VisualTestContext,
) -> (Rc<RefCell<Vec<PickerEvent>>>, gpui::Subscription) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let subscription = cx.update(|window, app| {
        window.subscribe(picker, app, move |_, event, _, _| {
            observed.borrow_mut().push(event.clone());
        })
    });
    (events, subscription)
}
