#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::syntax_highlighting;
}

#[path = "../src/shell/render/code_panel_syntax.rs"]
mod code_panel_syntax;

use std::{cell::RefCell, rc::Rc};

use syntax_highlighting::{SyntaxHighlight, SyntaxHighlightCache, SyntaxHighlightRequest};

fn token_roles(highlight: &SyntaxHighlight) -> Vec<syntax_highlighting::SyntaxTokenRole> {
    highlight
        .tokens()
        .iter()
        .map(|token| token.role())
        .collect()
}

#[test]
fn shared_code_panel_syntax_boundary_routes_markdown_aliases_for_non_transcript_owners() {
    let cache = Rc::new(RefCell::new(SyntaxHighlightCache::new(8, 4096)));
    let owner_id = "diagnostic-command-panel:1";
    let mut scheduled = Vec::<SyntaxHighlightRequest>::new();

    let pending = code_panel_syntax::resolve_code_panel_syntax_highlight(
        &cache,
        owner_id,
        "# heading",
        Some("md linenos"),
        |request| scheduled.push(request),
    );

    assert!(pending.is_plain());
    assert_eq!(scheduled.len(), 1);

    let completion = scheduled.pop().expect("Markdown lookup should schedule");
    let result = cache
        .borrow_mut()
        .complete_highlight(completion.highlight());
    assert!(result.display_changed);
    assert!(!result.stale);

    let mut rescheduled = Vec::<SyntaxHighlightRequest>::new();
    let ready = code_panel_syntax::resolve_code_panel_syntax_highlight(
        &cache,
        owner_id,
        "# heading",
        Some("markdown"),
        |request| rescheduled.push(request),
    );

    assert!(rescheduled.is_empty());
    assert_eq!(
        token_roles(ready.as_ref()),
        vec![syntax_highlighting::SyntaxTokenRole::MarkupHeadingMarker]
    );
}

#[test]
fn shared_code_panel_syntax_boundary_keeps_unregistered_labels_plain() {
    let cache = Rc::new(RefCell::new(SyntaxHighlightCache::new(8, 4096)));
    let owner_id = "standalone-code-panel:1";
    let mut scheduled = Vec::<SyntaxHighlightRequest>::new();
    let pending = code_panel_syntax::resolve_code_panel_syntax_highlight(
        &cache,
        owner_id,
        "# heading",
        Some("markdown"),
        |request| scheduled.push(request),
    );
    assert!(pending.is_plain());
    let completion = scheduled.pop().expect("Markdown lookup should schedule");
    assert!(
        cache
            .borrow_mut()
            .complete_highlight(completion.highlight())
            .display_changed
    );
    assert_eq!(cache.borrow().stats().entries, 1);

    for label in [None, Some(""), Some("rust"), Some("not a language")] {
        let mut plain_scheduled = Vec::<SyntaxHighlightRequest>::new();
        let highlight = code_panel_syntax::resolve_code_panel_syntax_highlight(
            &cache,
            owner_id,
            "# heading",
            label,
            |request| plain_scheduled.push(request),
        );

        assert!(highlight.is_plain());
        assert!(plain_scheduled.is_empty());
        assert_eq!(cache.borrow().stats().entries, 0);
    }
}
