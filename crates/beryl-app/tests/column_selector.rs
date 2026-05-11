#[path = "../src/shell/column_selector.rs"]
mod column_selector;

use column_selector::{
    ColumnSelectorKeyboardIntent, ColumnSelectorScrollState, ColumnSelectorState,
    keyboard_intent_for_keystroke,
};
use gpui::{point, px};

#[test]
fn column_selector_truncates_trail_when_selecting_from_an_earlier_column() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");

    assert!(selector.select_row(0, "child", Some("child")));
    assert!(selector.select_row(1, "grandchild", Some("grandchild")));
    assert_eq!(column_roots(&selector), vec!["root", "child", "grandchild"]);

    assert!(selector.select_row(0, "peer", Some("peer")));

    assert_eq!(column_roots(&selector), vec!["root", "peer"]);
    assert_eq!(selector.columns()[0].selection(), Some(&"peer"));
}

#[test]
fn column_selector_terminal_selection_does_not_open_a_new_column() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");

    assert!(selector.select_row(0, "terminal", None));

    assert_eq!(column_roots(&selector), vec!["root"]);
    assert_eq!(selector.columns()[0].selection(), Some(&"terminal"));
}

#[test]
fn column_selector_reports_change_when_reselection_clears_downstream_selection() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");

    assert!(selector.select_row(0, "child", Some("child")));
    assert!(selector.select_row(1, "grandchild", None));

    assert!(selector.select_row(0, "child", Some("child")));
    assert_eq!(column_roots(&selector), vec!["root", "child"]);
    assert_eq!(selector.columns()[1].selection(), None);
}

#[test]
fn column_selector_preserves_per_column_expansion_until_the_column_is_dropped() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");

    assert!(selector.columns_mut()[0].toggle_expansion(&"root-row", true));
    assert!(selector.select_row(0, "child", Some("child")));
    assert!(!selector.columns()[0].is_expanded(&"root-row", true));

    assert!(selector.columns_mut()[1].toggle_expansion(&"child-row", true));
    assert!(selector.select_row(0, "peer", Some("peer")));

    assert_eq!(column_roots(&selector), vec!["root", "peer"]);
    assert!(!selector.columns()[0].is_expanded(&"root-row", true));
}

#[test]
fn column_selector_replace_trail_preserves_expansion_for_matching_column_keys() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");

    assert!(selector.columns_mut()[0].toggle_expansion(&"root-row", true));
    assert!(selector.select_row(0, "child", Some("child")));
    assert!(selector.columns_mut()[1].toggle_expansion(&"child-row", true));

    selector.replace_trail_preserving_expansion([("root", Some("peer")), ("peer", None)]);

    assert_eq!(column_roots(&selector), vec!["root", "peer"]);
    assert_eq!(selector.columns()[0].selection(), Some(&"peer"));
    assert!(!selector.columns()[0].is_expanded(&"root-row", true));
}

#[test]
fn column_selector_scroll_handles_reconcile_with_current_column_trail() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");
    selector.select_row(0, "child", Some("child"));
    let mut scroll = ColumnSelectorScrollState::new();

    scroll.reconcile(selector.columns());
    assert_eq!(scroll_keys(&scroll), vec!["root", "child"]);

    selector.select_row(0, "peer", Some("peer"));
    scroll.reconcile(selector.columns());

    assert_eq!(scroll_keys(&scroll), vec!["root", "peer"]);
    assert!(scroll.column_handle(0).is_some());
    assert!(scroll.column_handle(1).is_some());
    assert!(scroll.column_handle(2).is_none());
}

#[test]
fn column_selector_scroll_handles_preserve_offsets_for_matching_column_keys() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");
    selector.select_row(0, "child", Some("child"));
    let mut scroll = ColumnSelectorScrollState::new();

    scroll.reconcile(selector.columns());
    let root_handle = scroll.column_handle(0).unwrap();
    let child_handle = scroll.column_handle(1).unwrap();
    root_handle.set_offset(point(px(0.0), px(-24.0)));
    child_handle.set_offset(point(px(0.0), px(-48.0)));

    scroll.reconcile(selector.columns());

    assert_eq!(
        scroll.column_handle(0).unwrap().offset(),
        root_handle.offset()
    );
    assert_eq!(
        scroll.column_handle(1).unwrap().offset(),
        child_handle.offset()
    );

    selector.select_row(0, "peer", Some("peer"));
    scroll.reconcile(selector.columns());

    assert_eq!(
        scroll.column_handle(0).unwrap().offset(),
        root_handle.offset()
    );
    assert_eq!(
        scroll.column_handle(1).unwrap().offset(),
        point(px(0.0), px(0.0))
    );
}

#[test]
fn column_selector_scroll_handles_preserve_offsets_when_matching_key_moves_index() {
    let mut selector = ColumnSelectorState::<&str, &str, &str>::from_root("root");
    selector.select_row(0, "child", Some("child"));
    selector.select_row(1, "grandchild", Some("grandchild"));
    let mut scroll = ColumnSelectorScrollState::new();

    scroll.reconcile(selector.columns());
    let grandchild_handle = scroll.column_handle(2).unwrap();
    grandchild_handle.set_offset(point(px(0.0), px(-72.0)));

    selector
        .replace_trail_preserving_expansion([("root", Some("grandchild")), ("grandchild", None)]);
    scroll.reconcile(selector.columns());

    assert_eq!(scroll_keys(&scroll), vec!["root", "grandchild"]);
    assert_eq!(
        scroll.column_handle(1).unwrap().offset(),
        grandchild_handle.offset()
    );
}

#[test]
fn column_selector_keyboard_intents_map_navigation_keys() {
    assert_eq!(
        keyboard_intent_for_keystroke("left"),
        Some(ColumnSelectorKeyboardIntent::PreviousColumn)
    );
    assert_eq!(
        keyboard_intent_for_keystroke("enter"),
        Some(ColumnSelectorKeyboardIntent::Activate)
    );
    assert_eq!(keyboard_intent_for_keystroke("ctrl-shift-g"), None);
}

fn column_roots(
    selector: &ColumnSelectorState<&'static str, &'static str, &'static str>,
) -> Vec<&'static str> {
    selector
        .columns()
        .iter()
        .map(|column| *column.root_key())
        .collect()
}

fn scroll_keys(scroll: &ColumnSelectorScrollState<&'static str>) -> Vec<&'static str> {
    scroll.column_keys().copied().collect()
}
