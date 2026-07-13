const SYNDIC_TRANSCRIPT_PANEL_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/panel.rs");
const SYNDIC_TRANSCRIPT_COMMAND_SOURCE: &str =
    include_str!("../src/shell/syndic_transcript/command.rs");

#[test]
fn empty_resident_host_reports_transcript_commands_unavailable() {
    assert!(SYNDIC_TRANSCRIPT_PANEL_SOURCE.contains("unavailable_command(&self"));
    assert!(SYNDIC_TRANSCRIPT_COMMAND_SOURCE.contains("Unavailable(DisabledTranscriptCommand)"));
    assert!(SYNDIC_TRANSCRIPT_COMMAND_SOURCE.contains("Self::Unavailable"));
    assert!(
        SYNDIC_TRANSCRIPT_COMMAND_SOURCE
            .contains("resident transcript data is not available for this command")
    );
}
