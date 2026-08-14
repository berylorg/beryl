use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::shell::syntax_highlighting::{
    SyntaxHighlight, SyntaxHighlightCache, SyntaxHighlightLookup, SyntaxHighlightRequest,
};

pub(crate) fn lookup_code_panel_syntax_highlight(
    cache: &Rc<RefCell<SyntaxHighlightCache>>,
    owner_id: &str,
    source: &str,
    syntax_label: Option<&str>,
) -> SyntaxHighlightLookup {
    cache.borrow_mut().lookup(owner_id, source, syntax_label)
}

pub(crate) fn resolve_code_panel_syntax_highlight(
    cache: &Rc<RefCell<SyntaxHighlightCache>>,
    owner_id: &str,
    source: &str,
    syntax_label: Option<&str>,
    schedule_request: impl FnOnce(SyntaxHighlightRequest),
) -> Arc<SyntaxHighlight> {
    let lookup = lookup_code_panel_syntax_highlight(cache, owner_id, source, syntax_label);
    if let Some(request) = lookup.highlight_request {
        schedule_request(request);
    }
    lookup.highlight
}
