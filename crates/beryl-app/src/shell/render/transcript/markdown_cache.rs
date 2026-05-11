use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::{App, AsyncApp, Entity};

use crate::shell::transcript_markdown::{
    ParsedTranscriptMarkdown, TranscriptMarkdownCache, TranscriptMarkdownCacheKey,
    TranscriptMarkdownParseRequest,
};

use super::TranscriptPanel;

#[derive(Clone)]
pub(super) struct TranscriptMarkdownRenderContext {
    cache: Rc<RefCell<TranscriptMarkdownCache>>,
    panel: Entity<TranscriptPanel>,
}

impl TranscriptMarkdownRenderContext {
    pub(super) fn new(
        cache: Rc<RefCell<TranscriptMarkdownCache>>,
        panel: Entity<TranscriptPanel>,
    ) -> Self {
        Self { cache, panel }
    }

    pub(super) fn markdown_for(
        &self,
        key: TranscriptMarkdownCacheKey,
        source: &str,
        cx: &mut App,
    ) -> Arc<ParsedTranscriptMarkdown> {
        let lookup = self.cache.borrow_mut().lookup(key, source);
        if let Some(request) = lookup.parse_request {
            schedule_markdown_parse(self.panel.clone(), request, cx);
        }
        lookup.markdown
    }
}

fn schedule_markdown_parse(
    panel: Entity<TranscriptPanel>,
    request: TranscriptMarkdownParseRequest,
    cx: &mut App,
) {
    let parse_task = cx
        .background_executor()
        .spawn(async move { request.parse() });
    cx.spawn(move |cx: &mut AsyncApp| {
        let mut cx = cx.clone();
        async move {
            let completion = parse_task.await;
            let _ = panel.update(&mut cx, |view, cx| {
                let result = view.markdown_cache.borrow_mut().complete_parse(completion);
                if let Some(request) = result.follow_up_request {
                    schedule_markdown_parse(cx.entity(), request, cx);
                }
                if result.display_changed {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}
