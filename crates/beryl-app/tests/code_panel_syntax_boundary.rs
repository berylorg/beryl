#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::syntax_highlighting;
}

#[path = "../src/shell/render/code_panel_syntax.rs"]
mod code_panel_syntax;
#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use std::{cell::RefCell, rc::Rc, sync::Arc};

use syntax_highlighting::{
    SyntaxHighlight, SyntaxHighlightCache, SyntaxHighlightRequest, SyntaxLanguage, SyntaxTokenRole,
};

fn token_roles(highlight: &SyntaxHighlight) -> Vec<syntax_highlighting::SyntaxTokenRole> {
    highlight
        .tokens()
        .iter()
        .map(|token| token.role())
        .collect()
}

fn assert_roles_include(highlight: &SyntaxHighlight, expected: &[SyntaxTokenRole]) {
    let roles = token_roles(highlight);
    for role in expected {
        assert!(roles.contains(role), "expected role {role:?} in {roles:?}");
    }
}

fn complete_boundary_highlight(
    cache: &Rc<RefCell<SyntaxHighlightCache>>,
    owner_id: &str,
    source: &str,
    label: Option<&str>,
) -> Arc<SyntaxHighlight> {
    let mut scheduled = Vec::<SyntaxHighlightRequest>::new();
    let pending = code_panel_syntax::resolve_code_panel_syntax_highlight(
        cache,
        owner_id,
        source,
        label,
        |request| scheduled.push(request),
    );

    assert!(pending.is_plain());
    assert_eq!(scheduled.len(), 1);

    let completion = scheduled.pop().expect("registered lookup should schedule");
    let result = cache
        .borrow_mut()
        .complete_highlight(completion.highlight());
    assert!(result.display_changed);
    assert!(!result.stale);

    let mut rescheduled = Vec::<SyntaxHighlightRequest>::new();
    let ready = code_panel_syntax::resolve_code_panel_syntax_highlight(
        cache,
        owner_id,
        source,
        label,
        |request| rescheduled.push(request),
    );

    assert!(rescheduled.is_empty());
    ready
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
fn shared_code_panel_syntax_boundary_routes_registered_languages_for_all_panel_owner_shapes() {
    let cache = Rc::new(RefCell::new(SyntaxHighlightCache::new(32, 4096)));
    let transcript_owner =
        transcript_markdown::markdown_code_panel_id("row-a", "item:answer", "b0");

    let cases: &[(&str, &str, SyntaxLanguage, &[SyntaxTokenRole])] = &[
        (
            "markdown",
            "# heading",
            SyntaxLanguage::Markdown,
            &[SyntaxTokenRole::MarkupHeadingMarker],
        ),
        (
            "json",
            r#"{"same": true}"#,
            SyntaxLanguage::Json,
            &[SyntaxTokenRole::SyntaxKey, SyntaxTokenRole::SyntaxBoolean],
        ),
        (
            "jsonl",
            "{\"same\": true}\nfalse",
            SyntaxLanguage::Jsonl,
            &[SyntaxTokenRole::SyntaxKey, SyntaxTokenRole::SyntaxBoolean],
        ),
        (
            "ndjson",
            "{\"same\": true}\nfalse",
            SyntaxLanguage::Jsonl,
            &[SyntaxTokenRole::SyntaxKey, SyntaxTokenRole::SyntaxBoolean],
        ),
        (
            "toml",
            "same = true",
            SyntaxLanguage::Toml,
            &[
                SyntaxTokenRole::SyntaxKey,
                SyntaxTokenRole::SyntaxAssignment,
                SyntaxTokenRole::SyntaxBoolean,
            ],
        ),
        (
            "ini",
            "[section]\nsame=true",
            SyntaxLanguage::WindowsIni,
            &[
                SyntaxTokenRole::SyntaxSectionHeader,
                SyntaxTokenRole::SyntaxKey,
                SyntaxTokenRole::SyntaxAssignment,
                SyntaxTokenRole::SyntaxString,
            ],
        ),
    ];

    for (label, source, language, expected_roles) in cases {
        for owner_id in [
            format!("standalone-code-panel:{label}"),
            format!("{transcript_owner}:{label}"),
        ] {
            let highlight =
                complete_boundary_highlight(&cache, owner_id.as_str(), source, Some(label));
            assert_eq!(highlight.language(), Some(*language));
            assert_roles_include(highlight.as_ref(), expected_roles);
        }
    }
}

#[test]
fn shared_code_panel_syntax_boundary_keeps_unregistered_labels_plain() {
    let cache = Rc::new(RefCell::new(SyntaxHighlightCache::new(8, 4096)));
    let owner_id = "standalone-code-panel:1";
    let mut scheduled = Vec::<SyntaxHighlightRequest>::new();
    let pending = code_panel_syntax::resolve_code_panel_syntax_highlight(
        &cache,
        owner_id,
        "same = true",
        Some("toml"),
        |request| scheduled.push(request),
    );
    assert!(pending.is_plain());
    let completion = scheduled.pop().expect("TOML lookup should schedule");
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
            "same = true",
            label,
            |request| plain_scheduled.push(request),
        );

        assert!(highlight.is_plain());
        assert!(plain_scheduled.is_empty());
        assert_eq!(cache.borrow().stats().entries, 0);
    }
}
