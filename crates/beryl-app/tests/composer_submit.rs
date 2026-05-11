#[path = "../src/shell/composer_submit.rs"]
mod composer_submit;

#[test]
fn accepted_composer_draft_preserves_original_text_for_submission() {
    let draft = "  keep leading and trailing spaces  \nsecond line";

    assert_eq!(
        composer_submit::accepted_composer_draft(draft).as_deref(),
        Some(draft)
    );
}

#[test]
fn whitespace_only_composer_draft_is_rejected_without_clearing() {
    assert_eq!(composer_submit::accepted_composer_draft(" \n\t "), None);
}
