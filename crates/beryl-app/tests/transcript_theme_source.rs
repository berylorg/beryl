const SHELL_SOURCE: &str = include_str!("../src/shell.rs");
const CONVERSATION_RENDER_SOURCE: &str = include_str!("../src/shell/render/conversation.rs");
const SYNDIC_TRANSCRIPT_PANEL_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/panel.rs");
const SYNDIC_TRANSCRIPT_HOST_SOURCE: &str = include_str!("../src/shell/syndic_transcript/host.rs");
const SYNDIC_TRANSCRIPT_CORE_SOURCE: &str = include_str!("../src/shell/syndic_transcript/core.rs");
const SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/snapshot.rs");

#[test]
fn visible_transcript_panel_uses_syndic_host_boundary() {
    let legacy_bind_keys = concat!("render", "::", "transcript", "::", "bind_keys(cx)");
    let legacy_panel = concat!("render", "::", "transcript", "::", "TranscriptPanel");

    assert!(SHELL_SOURCE.contains("mod syndic_transcript;"));
    assert!(
        SHELL_SOURCE.contains("transcript_panel: Entity<syndic_transcript::SyndicTranscriptPanel>")
    );
    assert!(
        SHELL_SOURCE.contains("cx.new(|cx| syndic_transcript::SyndicTranscriptPanel::new(cx))")
    );
    assert!(!SHELL_SOURCE.contains(legacy_bind_keys));
    assert!(!SHELL_SOURCE.contains(legacy_panel));

    assert!(
        CONVERSATION_RENDER_SOURCE
            .contains("use crate::shell::syndic_transcript::SyndicTranscriptPanel;")
    );
    assert!(
        CONVERSATION_RENDER_SOURCE.contains("transcript_panel: &Entity<SyndicTranscriptPanel>")
    );
    assert!(CONVERSATION_RENDER_SOURCE.contains("AnyView::from(transcript_panel.clone())"));
    assert!(!CONVERSATION_RENDER_SOURCE.contains(legacy_panel));
}

#[test]
fn resident_host_snapshot_is_theme_independent_and_empty_by_default() {
    assert!(SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("host: SyndicTranscriptHost"));
    assert!(SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("SyndicTranscriptHost::empty()"));
    assert!(SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("self.host.snapshot()"));
    assert!(SYNDIC_TRANSCRIPT_HOST_SOURCE.contains("core: ResidentTranscriptCore"));
    assert!(SYNDIC_TRANSCRIPT_HOST_SOURCE.contains("ResidentTranscriptCore::empty()"));
    assert!(SYNDIC_TRANSCRIPT_CORE_SOURCE.contains("ResidentTranscriptSnapshot::empty()"));
    assert!(SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE.contains("enum ResidentTranscriptSnapshotState"));
    assert!(SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE.contains("Empty,"));
    assert!(SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE.contains("records: Vec<ResidentPresentationRecord>"));

    for source in [
        SYNDIC_TRANSCRIPT_PANEL_SOURCE,
        SYNDIC_TRANSCRIPT_HOST_SOURCE,
        SYNDIC_TRANSCRIPT_CORE_SOURCE,
        SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE,
    ] {
        assert!(!source.contains("TranscriptTheme"));
        assert!(!source.contains("from_active_theme"));
        assert!(!source.contains("active_theme.lock"));
        assert!(!source.contains("theme_candidate"));
        assert!(!source.contains("syndic_storage"));
        assert!(!source.contains("syndic-storage"));
        assert!(!source.contains("render::transcript"));
    }
}
