use super::*;
use crate::model_selection::test_support::Fixture;
use crate::widgets::anchored_context_menu::AnchoredContextMenu;
use gpui::{AppContext, IntoElement, Render, div};

struct Host;
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn reasoning_pointer_from_prior_selected_option_cannot_activate_current_option(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new();
    let (_query, page) = fixture.first_page(2, None);
    let mut collection = ModelMenuCollection::new();
    collection.install(0, Arc::new(page), Some("model-0"));
    let values = ModelDefaults {
        model: Some("model-0".into()),
        reasoning: None,
    };
    let efforts = vec![ModelReasoningEffort::Low];
    let rows = ModelMenuRows::new(&collection, &values, false, None, efforts.clone());
    let index = rows.reasoning_start();
    let previous = rows.row(index);
    let previous_id = previous.id.unwrap();
    assert_eq!(previous.selector.as_ref(), "model-reasoning-low");
    collection.selected = Some((1, collection.row(1).unwrap().1.clone()));
    let values = ModelDefaults {
        model: Some("model-1".into()),
        reasoning: None,
    };
    let rows = ModelMenuRows::new(&collection, &values, false, None, efforts);
    let current = rows.row(index);
    let current_id = current.id.unwrap();
    assert_ne!(previous_id, current_id);
    assert_eq!(current.selector.as_ref(), "model-reasoning-low");
    assert_eq!(rows.index_of(&previous_id), None);
    assert_eq!(rows.index_of(&current_id), Some(index));
    let host = cx.update(|app| {
        app.open_window(gpui::WindowOptions::default(), |_, app| app.new(|_| Host))
            .unwrap()
    });
    host.update(cx, |_, window, cx| {
        let mut menu = AnchoredContextMenu::open(1, window, cx);
        let realized = [rows.row(index)];
        menu.realize(&realized, index..index + 1, window);
        assert!(menu.input(MenuEvent::Pointer { invocation: 1, index, id: previous_id }, &rows).is_empty());
        assert!(matches!(&menu.input(MenuEvent::Pointer { invocation: 1, index, id: current_id.clone() }, &rows)[..], [MenuIntent::Focus(target), MenuIntent::Activate { index: activated, id }] if *target == index && *activated == index && id == &current_id));
    }).unwrap();
    assert_eq!(fixture.server.requests("model/list").len(), 1);
    assert!(fixture.server.requests("turn/start").is_empty());
}
