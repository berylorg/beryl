const SYNDIC_TRANSCRIPT_PANEL_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/panel.rs");
const SYNDIC_TRANSCRIPT_HOST_SOURCE: &str = include_str!("../src/shell/syndic_transcript/host.rs");
const SYNDIC_TRANSCRIPT_CORE_SOURCE: &str = include_str!("../src/shell/syndic_transcript/core.rs");
const SYNDIC_TRANSCRIPT_SNAPSHOT_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/snapshot.rs");

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
