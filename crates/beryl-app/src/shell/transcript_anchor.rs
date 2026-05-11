#[path = "transcript_anchor/markdown_layout.rs"]
mod markdown_layout;
#[path = "transcript_anchor/window_measure.rs"]
mod window_measure;

use gpui::{Pixels, Window, px};

use crate::AppearanceSettings;

use self::markdown_layout::prompt_markdown_layout_from_plan;
use self::window_measure::WindowPromptMeasurer;
use super::transcript_markdown::BlockRenderPlan;

const FIRST_TURN_TOP_PADDING: f32 = 16.0;
const TURN_ROW_HORIZONTAL_PADDING: f32 = 24.0;
const USER_PROMPT_BLOCK_BORDER: f32 = 1.0;
const USER_PROMPT_BLOCK_PADDING: f32 = 12.0;
const TRAILING_SLACK_PAINT_GUARD: f32 = 1.0;
const USER_PROMPT_HORIZONTAL_CHROME: f32 = TURN_ROW_HORIZONTAL_PADDING
    + (USER_PROMPT_BLOCK_BORDER * 2.0)
    + (USER_PROMPT_BLOCK_PADDING * 2.0);
const USER_PROMPT_VERTICAL_CHROME: f32 =
    (USER_PROMPT_BLOCK_BORDER * 2.0) + (USER_PROMPT_BLOCK_PADDING * 2.0);
const TURN_CARD_BLOCK_GAP: f32 = 12.0;
const MARKDOWN_NORMAL_BLOCK_GAP: f32 = 8.0;
const MARKDOWN_TIGHT_BLOCK_GAP: f32 = 4.0;
const MARKDOWN_HEADING_BOTTOM_PADDING: f32 = 4.0;
const MARKDOWN_LIST_MARKER_WIDTH: f32 = 32.0;
const MARKDOWN_LIST_MARKER_GAP: f32 = 8.0;
const MARKDOWN_QUOTE_BORDER: f32 = 2.0;
const MARKDOWN_QUOTE_PADDING_LEFT: f32 = 12.0;
const MARKDOWN_QUOTE_PADDING_VERTICAL: f32 = 4.0;
const MARKDOWN_THEMATIC_BREAK_HEIGHT: f32 = 1.0;
const MARKDOWN_THEMATIC_BREAK_MARGIN_VERTICAL: f32 = 4.0;
const CODE_FONT_FAMILY: &str = "Consolas";
const CODE_FONT_SIZE_REM: f32 = 0.875;
const CODE_HEADER_FONT_SIZE_REM: f32 = 0.75;
const CODE_PANEL_BORDER: f32 = 1.0;
const CODE_PANEL_CONTENT_PADDING: f32 = 12.0;
const CODE_PANEL_HEADER_VERTICAL_PADDING: f32 = 8.0;
const CODE_PANEL_HEADER_CONTENT_BORDER: f32 = 1.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSubmitAnchor {
    turn_index: usize,
    fragment_index: usize,
    user_input: String,
    force_viewport: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSubmitAnchorSnapshot {
    pub(crate) turn_index: usize,
    pub(crate) fragment_index: usize,
    pub(crate) user_input: String,
    pub(crate) force_viewport: bool,
}

impl TranscriptSubmitAnchor {
    pub(crate) fn new(turn_index: usize, fragment_index: usize, user_input: String) -> Self {
        Self {
            turn_index,
            fragment_index,
            user_input,
            force_viewport: true,
        }
    }

    pub(crate) fn passive(turn_index: usize, fragment_index: usize, user_input: String) -> Self {
        Self {
            turn_index,
            fragment_index,
            user_input,
            force_viewport: false,
        }
    }

    pub(crate) fn snapshot(&self) -> TranscriptSubmitAnchorSnapshot {
        TranscriptSubmitAnchorSnapshot {
            turn_index: self.turn_index,
            fragment_index: self.fragment_index,
            user_input: self.user_input.clone(),
            force_viewport: self.force_viewport,
        }
    }

    pub(crate) fn release_forced_viewport(&mut self) -> bool {
        let was_forced = self.force_viewport;
        self.force_viewport = false;
        was_forced
    }

    pub(crate) fn shift_turn_index(&mut self, amount: usize) {
        self.turn_index = self.turn_index.saturating_add(amount);
    }
}

pub(crate) fn prompt_last_line_top_offset(
    snapshot: &TranscriptSubmitAnchorSnapshot,
    preceding_prompt_plans: &[&BlockRenderPlan],
    prompt_plan: &BlockRenderPlan,
    transcript_width: Pixels,
    appearance: &AppearanceSettings,
    transcript_code_columns: usize,
    window: &mut Window,
) -> Pixels {
    let prompt_width = prompt_text_width(transcript_width);
    let mut measurer = WindowPromptMeasurer::new(appearance, window);
    let layout = prompt_markdown_layout_from_plan(
        prompt_plan,
        prompt_width,
        transcript_code_columns,
        &mut measurer,
    );

    let preceding_height = preceding_prompt_plans
        .iter()
        .map(|plan| {
            let layout = prompt_markdown_layout_from_plan(
                plan,
                prompt_width,
                transcript_code_columns,
                &mut measurer,
            );
            prompt_block_outer_height(layout.height) + px(TURN_CARD_BLOCK_GAP)
        })
        .fold(px(0.0), |total, height| total + height);

    prompt_content_top_offset(snapshot.turn_index) + preceding_height + layout.last_line_top
}

pub(crate) fn trailing_scroll_slack(
    viewport_height: Pixels,
    measured_content_below_anchor: Option<Pixels>,
) -> Pixels {
    let max_spacer = (viewport_height.max(px(0.0)) - px(TRAILING_SLACK_PAINT_GUARD)).max(px(0.0));
    let Some(content_below_anchor) = measured_content_below_anchor else {
        return max_spacer;
    };

    (viewport_height - content_below_anchor.max(px(0.0)))
        .max(px(0.0))
        .min(max_spacer)
}

pub(crate) fn transcript_list_item_count(turn_count: usize) -> usize {
    turn_count
}

pub(crate) fn release_forced_submit_anchor(anchor: &mut Option<TranscriptSubmitAnchor>) -> bool {
    anchor
        .as_mut()
        .is_some_and(TranscriptSubmitAnchor::release_forced_viewport)
}

fn prompt_text_width(transcript_width: Pixels) -> Pixels {
    (transcript_width - px(USER_PROMPT_HORIZONTAL_CHROME)).max(px(1.0))
}

#[cfg(test)]
fn prompt_last_line_top_offset_from_counts(
    turn_index: usize,
    line_counts: &[usize],
    line_height: Pixels,
) -> Pixels {
    let line_count_before_last = line_counts.iter().copied().sum::<usize>().saturating_sub(1);
    let first_turn_top_padding = if turn_index == 0 {
        px(FIRST_TURN_TOP_PADDING)
    } else {
        px(0.0)
    };

    first_turn_top_padding
        + prompt_block_content_top_offset()
        + (line_height * line_count_before_last as f32)
}

fn prompt_content_top_offset(turn_index: usize) -> Pixels {
    let first_turn_top_padding = if turn_index == 0 {
        px(FIRST_TURN_TOP_PADDING)
    } else {
        px(0.0)
    };

    first_turn_top_padding + prompt_block_content_top_offset()
}

fn prompt_block_content_top_offset() -> Pixels {
    px(USER_PROMPT_BLOCK_BORDER + USER_PROMPT_BLOCK_PADDING)
}

fn prompt_block_outer_height(content_height: Pixels) -> Pixels {
    content_height + px(USER_PROMPT_VERTICAL_CHROME)
}

fn prompt_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines = normalized
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod test_support {
    use gpui::Pixels;

    use super::super::transcript_markdown::InlineRenderLine;
    use super::markdown_layout::{AnchorBlockRole, PromptTextMeasurer, prompt_markdown_layout};
    use super::prompt_content_top_offset;

    pub(crate) fn prompt_last_line_top_offset_from_counts(
        turn_index: usize,
        paragraph_line_counts: &[usize],
        line_height: Pixels,
    ) -> Pixels {
        super::prompt_last_line_top_offset_from_counts(
            turn_index,
            paragraph_line_counts,
            line_height,
        )
    }

    pub(crate) fn prompt_lines(text: &str) -> Vec<String> {
        super::prompt_lines(text)
    }

    pub(crate) fn prompt_last_line_top_offset_from_markdown_no_wrap(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
    ) -> Pixels {
        let mut measurer = FixedPromptMeasurer {
            line_height,
            heading_line_height,
            code_line_height,
            code_header_line_height,
            inline_columns: None,
            code_columns: transcript_code_columns,
        };
        prompt_content_top_offset(turn_index)
            + prompt_markdown_layout(source, prompt_width, transcript_code_columns, &mut measurer)
                .last_line_top
    }

    pub(crate) fn prompt_last_line_top_offset_from_markdown_columns(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        inline_columns: usize,
        code_columns: usize,
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
    ) -> Pixels {
        let mut measurer = FixedPromptMeasurer {
            line_height,
            heading_line_height,
            code_line_height,
            code_header_line_height,
            inline_columns: Some(inline_columns.max(1)),
            code_columns: code_columns.max(1),
        };
        prompt_content_top_offset(turn_index)
            + prompt_markdown_layout(source, prompt_width, transcript_code_columns, &mut measurer)
                .last_line_top
    }

    struct FixedPromptMeasurer {
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
        inline_columns: Option<usize>,
        code_columns: usize,
    }

    impl PromptTextMeasurer for FixedPromptMeasurer {
        fn inline_visual_line_count(
            &mut self,
            line: &InlineRenderLine,
            _role: AnchorBlockRole,
            _wrap_width: Pixels,
        ) -> usize {
            let Some(columns) = self.inline_columns else {
                return 1;
            };
            let len = line
                .fragments
                .iter()
                .map(|fragment| fragment.text.chars().count())
                .sum::<usize>();
            len.max(1).div_ceil(columns)
        }

        fn block_line_height(&self, role: AnchorBlockRole) -> Pixels {
            match role {
                AnchorBlockRole::Conversation => self.line_height,
                AnchorBlockRole::Heading { .. } => self.heading_line_height,
            }
        }

        fn code_line_height(&self) -> Pixels {
            self.code_line_height
        }

        fn code_header_line_height(&self) -> Pixels {
            self.code_header_line_height
        }

        fn code_columns_for_width(&mut self, _wrap_width: Pixels) -> usize {
            self.code_columns
        }
    }
}
