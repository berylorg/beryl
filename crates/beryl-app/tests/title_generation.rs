#[path = "../src/title_generation.rs"]
mod title_generation;

#[test]
fn title_generation_prefers_normalized_user_input() {
    let title = title_generation::derive_short_title_from_turn(
        "### 1. Fix untitled thread names in Beryl and verify restart behavior",
        "The change is implemented.",
    );

    assert_eq!(
        title.as_deref(),
        Some("Fix untitled thread names in Beryl and")
    );
}

#[test]
fn title_generation_falls_back_to_assistant_text() {
    let title = title_generation::derive_short_title_from_turn(
        "```",
        "- Inspect backend thread-name propagation and add fallback metadata",
    );

    assert_eq!(
        title.as_deref(),
        Some("Inspect backend thread-name propagation and add fallback")
    );
}
