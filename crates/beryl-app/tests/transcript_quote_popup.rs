#[path = "../src/shell/transcript_quote_popup.rs"]
mod transcript_quote_popup;

use gpui::{Bounds, point, px, size};
use transcript_quote_popup::{
    TranscriptQuotePopupState, popup_height, popup_width, quote_popup_position,
    selection_bounds_union, selection_geometry_matches_viewport,
};

#[test]
fn popup_opens_and_repositions_for_selection() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(80.0), px(80.0)), size(px(120.0), px(24.0)));
    let scrolled_selection = Bounds::new(point(px(80.0), px(112.0)), size(px(120.0), px(24.0)));
    let mut popup = TranscriptQuotePopupState::default();

    assert!(popup.open_for_selection(selection, viewport));
    let first_position = popup.position();
    assert!(first_position.is_some());
    assert!(!popup.open_for_selection(selection, viewport));

    assert!(popup.open_for_selection(scrolled_selection, viewport));
    assert_ne!(popup.position(), first_position);
}

#[test]
fn popup_stays_available_for_same_selection_generation() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(80.0), px(80.0)), size(px(120.0), px(24.0)));
    let mut popup = TranscriptQuotePopupState::default();

    assert!(popup.open_for_selection(selection, viewport));
    let first_position = popup.position();

    assert!(!popup.open_for_selection(selection, viewport));
    assert_eq!(popup.position(), first_position);
}

#[test]
fn selection_mutation_closes_popup_and_allows_next_selection() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(80.0), px(80.0)), size(px(120.0), px(24.0)));
    let mut popup = TranscriptQuotePopupState::default();

    popup.open_for_selection(selection, viewport);
    assert!(popup.note_selection_mutated());
    assert_eq!(popup.position(), None);
    assert!(popup.open_for_selection(selection, viewport));
}

#[test]
fn popup_tracks_bounds_until_selection_changes() {
    let mut popup = TranscriptQuotePopupState::default();
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(80.0), px(80.0)), size(px(120.0), px(24.0)));
    let bounds = Bounds::new(
        point(px(90.0), px(38.0)),
        size(popup_width(), popup_height()),
    );

    popup.open_for_selection(selection, viewport);
    assert!(popup.set_popup_bounds(Some(bounds)));
    assert!(!popup.set_popup_bounds(Some(bounds)));

    assert!(popup.note_selection_mutated());
    assert!(!popup.set_popup_bounds(Some(bounds)));
}

#[test]
fn clear_selection_closes_popup_and_allows_next_selection() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(80.0), px(80.0)), size(px(120.0), px(24.0)));
    let mut popup = TranscriptQuotePopupState::default();

    popup.open_for_selection(selection, viewport);
    assert!(popup.clear_selection());
    assert_eq!(popup.position(), None);
    assert!(popup.open_for_selection(selection, viewport));
}

#[test]
fn position_prefers_above_selection_when_space_allows() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let selection = Bounds::new(point(px(100.0), px(100.0)), size(px(100.0), px(20.0)));

    let position = quote_popup_position(selection, viewport);

    assert_eq!(position.y, px(58.0));
    assert_eq!(position.x, px(113.0));
}

#[test]
fn position_falls_below_and_clamps_to_viewport() {
    let viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(150.0), px(120.0)));
    let selection = Bounds::new(point(px(120.0), px(8.0)), size(px(40.0), px(20.0)));

    let position = quote_popup_position(selection, viewport);

    assert_eq!(position.y, px(36.0));
    assert_eq!(position.x, px(68.0));
}

#[test]
fn selection_bounds_union_covers_all_ranges() {
    let bounds = selection_bounds_union([
        Bounds::new(point(px(20.0), px(40.0)), size(px(80.0), px(16.0))),
        Bounds::new(point(px(12.0), px(70.0)), size(px(48.0), px(16.0))),
        Bounds::new(point(px(90.0), px(60.0)), size(px(24.0), px(16.0))),
    ])
    .expect("union");

    assert_eq!(bounds.left(), px(12.0));
    assert_eq!(bounds.top(), px(40.0));
    assert_eq!(bounds.right(), px(114.0));
    assert_eq!(bounds.bottom(), px(86.0));
}

#[test]
fn selection_geometry_must_match_current_viewport_before_popup_placement() {
    let old_viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
    let current_viewport = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(240.0)));

    assert!(!selection_geometry_matches_viewport(
        Some(old_viewport),
        current_viewport,
    ));
    assert!(selection_geometry_matches_viewport(
        Some(current_viewport),
        current_viewport,
    ));
    assert!(!selection_geometry_matches_viewport(None, current_viewport));
}
