use super::*;

#[test]
fn diagnostic_projection_is_commandless_and_bounded_at_unicode_edges() {
    for length in [
        0,
        1,
        NOTICE_DETAIL_BYTES - 1,
        NOTICE_DETAIL_BYTES,
        NOTICE_DETAIL_BYTES + 1,
    ] {
        for suffix in ["", "é", "🦀"] {
            let source = format!("{}{suffix}", "x".repeat(length));
            let content = failure_content(&source);
            assert_eq!(content.title().as_str(), "Couldn't exit Beryl");
            assert_eq!(content.variant, NoticeVariant::Error);
            assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
            assert_eq!(content.commands().count(), 0);
            assert!(content.detail().as_str().len() <= NOTICE_DETAIL_BYTES);
            assert_eq!(
                content.detail().is_truncated(),
                source.len() > NOTICE_DETAIL_BYTES
            );
            if source.len() <= NOTICE_DETAIL_BYTES {
                assert_eq!(content.detail().as_str(), source);
            } else {
                assert!(content.detail().as_str().ends_with('…'));
            }
        }
    }
}

#[test]
fn diagnostic_formatting_stops_at_the_display_budget() {
    struct LongDiagnostic(std::cell::Cell<usize>);
    impl fmt::Display for LongDiagnostic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            loop {
                self.0.set(self.0.get() + 1);
                f.write_str("🦀")?;
                assert!(self.0.get() <= NOTICE_DETAIL_BYTES, "unbounded formatting");
            }
        }
    }
    let diagnostic = LongDiagnostic(std::cell::Cell::new(0));
    let content = failure_content(&diagnostic);
    assert!(content.detail().is_truncated());
    assert_eq!(diagnostic.0.get(), NOTICE_DETAIL_BYTES / 4 + 1);
}
