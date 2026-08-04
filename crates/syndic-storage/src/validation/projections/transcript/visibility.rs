pub(super) const fn is_transcript_visible(kind: crate::CanonicalItemKind) -> bool {
    matches!(
        kind,
        crate::CanonicalItemKind::UserInput
            | crate::CanonicalItemKind::AssistantMessage(_)
            | crate::CanonicalItemKind::ProviderText(_)
    )
}
