#[path = "../src/shell/transcript_quote.rs"]
mod transcript_quote;

use gpui_text_input::{TextInputOptions, TextInputState};
use transcript_quote::quote_insertion_for_draft;

#[test]
fn quote_insertion_prefixes_logical_lines_and_normalizes_newlines() {
    let insertion =
        quote_insertion_for_draft("", 0, "alpha\r\nbeta\rgamma\n").expect("quote insertion");

    assert_eq!(insertion.insertion_offset, 0);
    assert_eq!(insertion.inserted_text, "> alpha\n> beta\n> gamma\n\n");
    assert_eq!(
        insertion.next_insertion_offset,
        insertion.inserted_text.len()
    );
}

#[test]
fn quote_insertion_preserves_internal_blank_lines() {
    let insertion = quote_insertion_for_draft("", 0, "alpha\n\nbeta").expect("quote insertion");

    assert_eq!(insertion.inserted_text, "> alpha\n> \n> beta\n\n");
}

#[test]
fn quote_insertion_preserves_fenced_markdown_selection() {
    let insertion =
        quote_insertion_for_draft("", 0, "```rust\nfn main() {}\n```").expect("quote insertion");

    assert_eq!(
        insertion.inserted_text,
        "> ```rust\n> fn main() {}\n> ```\n\n"
    );
}

#[test]
fn quote_insertion_preserves_markdown_block_markers_from_selection() {
    let selection = "# Title\n\n> quote\n\n```rust\nfn main() {}\n```";
    let insertion = quote_insertion_for_draft("", 0, selection).expect("quote insertion");

    assert_eq!(
        insertion.inserted_text,
        "> # Title\n> \n> > quote\n> \n> ```rust\n> fn main() {}\n> ```\n\n"
    );
}

#[test]
fn quote_insertion_preserves_image_marker_copy_reference_text() {
    let insertion =
        quote_insertion_for_draft("", 0, "See [Image A] here").expect("quote insertion");

    assert_eq!(insertion.inserted_text, "> See [Image A] here\n\n");
}

#[test]
fn quote_insertion_adds_blank_line_spacing_around_existing_draft_text() {
    let draft = "intro\nbody";
    let insertion =
        quote_insertion_for_draft(draft, "intro".len(), "quoted").expect("quote insertion");
    let mut combined = draft.to_string();
    combined.insert_str(insertion.insertion_offset, &insertion.inserted_text);

    assert_eq!(insertion.inserted_text, "\n\n> quoted\n");
    assert_eq!(combined, "intro\n\n> quoted\n\nbody");
}

#[test]
fn quote_insertion_advances_for_repeated_quote_harvesting() {
    let first = quote_insertion_for_draft("", 0, "first").expect("first quote");
    let mut draft = String::new();
    draft.insert_str(first.insertion_offset, &first.inserted_text);

    let second =
        quote_insertion_for_draft(&draft, first.next_insertion_offset, "second").expect("second");
    draft.insert_str(second.insertion_offset, &second.inserted_text);

    assert_eq!(draft, "> first\n\n> second\n\n");
    assert_eq!(second.insertion_offset, first.next_insertion_offset);
    assert_eq!(second.next_insertion_offset, draft.len());
}

#[test]
fn quote_harvesting_uses_and_advances_the_shared_input_caret() {
    let mut input = TextInputState::new("intro\noutro".to_string(), TextInputOptions::multiline());

    assert!(input.move_to_offset("intro".len()));
    let first = quote_insertion_for_draft(input.text(), input.cursor_offset(), "first")
        .expect("first quote");
    assert!(
        input
            .insert_text_at_offset(first.insertion_offset, &first.inserted_text)
            .is_some()
    );
    assert_eq!(input.cursor_offset(), first.next_insertion_offset);

    let second = quote_insertion_for_draft(input.text(), input.cursor_offset(), "second")
        .expect("second quote");
    assert!(
        input
            .insert_text_at_offset(second.insertion_offset, &second.inserted_text)
            .is_some()
    );

    assert_eq!(input.text(), "intro\n\n> first\n\n> second\n\noutro");
    assert_eq!(input.cursor_offset(), second.next_insertion_offset);
}

#[test]
fn quote_insertion_clamps_offset_to_character_boundary() {
    let insertion = quote_insertion_for_draft("\u{e9}clair", 1, "quoted").expect("quote insertion");

    assert_eq!(insertion.insertion_offset, 0);
    assert_eq!(insertion.inserted_text, "> quoted\n\n");
}

#[test]
fn quote_insertion_rejects_empty_selection() {
    assert!(quote_insertion_for_draft("draft", 0, "").is_none());
}
