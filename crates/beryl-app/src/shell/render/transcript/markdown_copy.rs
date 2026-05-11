use crate::shell::transcript_markdown::{BlockRenderCode, InlineRenderLine};
use crate::shell::transcript_selection::{TranscriptLineCopyGroup, TranscriptLineCopyText};

pub(super) fn inline_line_copy_text(line: &InlineRenderLine) -> TranscriptLineCopyText {
    let mut copy_text = TranscriptLineCopyText::default();
    for fragment in &line.fragments {
        if let Some(copy_replacement) = &fragment.copy_replacement {
            copy_text.push_atomic_run(fragment.text.clone(), copy_replacement.clone());
        } else {
            copy_text.push_wrapped_run(
                fragment.text.clone(),
                fragment.copy_prefix.clone(),
                fragment.copy_suffix.clone(),
            );
        }
    }
    copy_text
}

pub(super) fn code_block_copy_group(
    id: impl Into<String>,
    code: &BlockRenderCode,
) -> TranscriptLineCopyGroup {
    TranscriptLineCopyGroup::new(
        id,
        code.copy_opening_fence.clone(),
        code.copy_closing_fence.clone(),
    )
}
