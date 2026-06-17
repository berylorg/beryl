use std::ops::Range;

use beryl_model::conversation::ConversationThreadId;

use crate::branch_bootstrap_core::parse_beryl_thread_link;

use super::transcript_markdown::InlineRenderLine;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptThreadLinkRange {
    thread_id: ConversationThreadId,
    display_range: Range<usize>,
}

impl TranscriptThreadLinkRange {
    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn display_range(&self) -> Range<usize> {
        self.display_range.clone()
    }
}

pub(crate) fn inline_thread_link_ranges(line: &InlineRenderLine) -> Vec<TranscriptThreadLinkRange> {
    let mut display_cursor = 0usize;
    let mut links = Vec::new();

    for fragment in &line.fragments {
        let display_range = display_cursor..display_cursor + fragment.text.len();
        display_cursor = display_range.end;
        if display_range.start == display_range.end {
            continue;
        }
        let Some(thread_id) = fragment
            .link_destination
            .as_deref()
            .and_then(parse_beryl_thread_link)
        else {
            continue;
        };
        push_thread_link_range(&mut links, thread_id, display_range);
    }

    links
}

fn push_thread_link_range(
    links: &mut Vec<TranscriptThreadLinkRange>,
    thread_id: ConversationThreadId,
    display_range: Range<usize>,
) {
    if let Some(previous) = links.last_mut()
        && previous.thread_id == thread_id
        && previous.display_range.end == display_range.start
    {
        previous.display_range.end = display_range.end;
        return;
    }

    links.push(TranscriptThreadLinkRange {
        thread_id,
        display_range,
    });
}
