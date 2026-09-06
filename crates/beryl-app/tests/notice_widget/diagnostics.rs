use super::support::*;
use beryl_app::main_window::{
    NoticeCommand, NoticeCommandId, NoticeContent, NoticeDismissal, NoticeVariant,
};

#[gpui::test]
fn diagnostics_are_content_free_and_redact_stable_owner_values(cx: &mut gpui::TestAppContext) {
    let mounted = Mounted::new(cx);
    let title = "private-title-7a6f";
    let detail = "private-detail-a83e";
    let command = "private-command-e6b1";
    let source = NoticeSource::new(
        17,
        NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            title,
            detail,
        )
        .with_commands(&[NoticeCommand::enabled(NoticeCommandId::new(701), command)])
        .expect("one bounded command"),
    );
    mounted.replace(Some(source.record()), cx);
    let diagnostic = format!("{:?}", mounted.diagnostics(cx));
    for secret in [title, detail, command] {
        assert!(
            !diagnostic.contains(secret),
            "diagnostics must not expose owner-supplied content or stable raw identifiers"
        );
    }
    assert!(diagnostic.contains("MainWindowNoticeDiagnosticKey(..)"));
}
