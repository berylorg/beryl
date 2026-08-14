pub(crate) fn accepted_composer_draft(draft: &str) -> Option<String> {
    (!draft.trim().is_empty()).then(|| draft.to_string())
}
