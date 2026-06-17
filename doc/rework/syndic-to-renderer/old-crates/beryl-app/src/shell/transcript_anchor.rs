#[path = "transcript_anchor/markdown_layout.rs"]
mod markdown_layout;
#[path = "transcript_anchor/window_measure.rs"]
mod window_measure;

use gpui::{Pixels, Window, px};

use self::markdown_layout::{PromptTextMeasurer, prompt_markdown_layout_from_plan};
use self::window_measure::WindowPromptMeasurer;
use super::transcript_markdown::BlockRenderPlan;

const FIRST_TURN_TOP_PADDING: f32 = 16.0;
const TURN_ROW_HORIZONTAL_PADDING: f32 = 24.0;
const USER_PROMPT_BLOCK_BORDER: f32 = 1.0;
const USER_PROMPT_BLOCK_PADDING: f32 = 12.0;
const TRAILING_SLACK_PAINT_GUARD: f32 = 1.0;
const FINAL_START_MIN_TOP_GUARD: f32 = 8.0;
const FINAL_START_LINE_HEIGHT_GUARD_RATIO: f32 = 0.25;
const USER_PROMPT_HORIZONTAL_CHROME: f32 = TURN_ROW_HORIZONTAL_PADDING
    + (USER_PROMPT_BLOCK_BORDER * 2.0)
    + (USER_PROMPT_BLOCK_PADDING * 2.0);
const USER_PROMPT_VERTICAL_CHROME: f32 =
    (USER_PROMPT_BLOCK_BORDER * 2.0) + (USER_PROMPT_BLOCK_PADDING * 2.0);
const TURN_CARD_BLOCK_GAP: f32 = 12.0;
const MARKDOWN_NORMAL_BLOCK_GAP: f32 = 8.0;
const MARKDOWN_TIGHT_BLOCK_GAP: f32 = 4.0;
const MARKDOWN_HEADING_BOTTOM_PADDING: f32 = 4.0;
const MARKDOWN_QUOTE_BORDER: f32 = 2.0;
const MARKDOWN_QUOTE_PADDING_LEFT: f32 = 12.0;
const MARKDOWN_QUOTE_PADDING_VERTICAL: f32 = 4.0;
const MARKDOWN_THEMATIC_BREAK_HEIGHT: f32 = 1.0;
const MARKDOWN_THEMATIC_BREAK_MARGIN_VERTICAL: f32 = 4.0;
const CODE_PANEL_BORDER: f32 = 1.0;
const CODE_PANEL_CONTENT_PADDING: f32 = 12.0;
const CODE_PANEL_HEADER_VERTICAL_PADDING: f32 = 8.0;
const CODE_PANEL_HEADER_CONTENT_BORDER: f32 = 1.0;
const CODE_PANEL_HEADER_MIN_CONTENT_HEIGHT: f32 = 32.0;
const CODE_PANEL_RESIZE_HANDLE_HEIGHT: f32 = 10.0;
const CODE_PANEL_VISIBLE_LINE_CAP: usize = 12;
const CODE_PANEL_RESIZABLE_CONTENT_VERTICAL_PADDING: f32 = 24.0;
const CODE_PANEL_MIN_HEIGHT: f32 = 64.0;
const CODE_PANEL_DEFAULT_MAX_HEIGHT: f32 = 360.0;
const RESPONSE_RUNWAY_LINES: f32 = 2.0;
const MIN_VISIBLE_PROMPT_LINES_WITH_RUNWAY: f32 = 2.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSubmitAnchor {
    turn_index: usize,
    row_identity: Option<String>,
    fragment_index: usize,
    user_input: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptSubmitAnchorSnapshot {
    pub(crate) turn_index: usize,
    pub(crate) row_identity: Option<String>,
    pub(crate) fragment_index: usize,
    pub(crate) user_input: String,
    pub(crate) viewport_action: TranscriptSubmitViewportAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptSubmitViewportAction {
    PromptReread,
    MaintainPromptRunway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptPromptAnchorKind {
    FragmentStart,
    FragmentTail,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptPromptFragmentGeometry {
    pub(crate) fragment_start_offset: Pixels,
    pub(crate) fragment_content_start_offset: Pixels,
    pub(crate) fragment_tail_offset: Pixels,
    pub(crate) last_visual_line_top_offset: Pixels,
    pub(crate) conversation_line_height: Pixels,
    visual_line_top_offsets: Vec<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptPromptViewportPlacement {
    pub(crate) scroll_offset: Pixels,
    pub(crate) virtual_runway: Pixels,
    pub(crate) anchor_kind: TranscriptPromptAnchorKind,
    pub(crate) prompt: TranscriptPromptFragmentGeometry,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptNarrativeItemGeometry {
    pub(crate) item_id: Option<String>,
    pub(crate) top_offset: Pixels,
    pub(crate) height: Pixels,
    pub(crate) first_line_height: Pixels,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptNarrativeTextRole {
    AssistantFinal,
    AssistantCommentary,
    AssistantReasoning,
}

pub(crate) struct TranscriptNarrativeAnchorThemes<'a> {
    pub(crate) prompt: &'a TranscriptAnchorTheme,
    pub(crate) assistant_final: &'a TranscriptAnchorTheme,
    pub(crate) assistant_commentary: &'a TranscriptAnchorTheme,
    pub(crate) assistant_reasoning: &'a TranscriptAnchorTheme,
}

#[allow(dead_code)]
pub(crate) enum TranscriptNarrativeBlockPlan {
    UserPrompt {
        plan: BlockRenderPlan,
    },
    AssistantMarkdown {
        item_id: String,
        plan: BlockRenderPlan,
        role: TranscriptNarrativeTextRole,
    },
    Anonymous {
        height: Pixels,
    },
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptCommentaryFollowGeometry {
    pub(crate) item_id: Option<String>,
    pub(crate) item_bottom_offset: Pixels,
    pub(crate) scroll_offset: Pixels,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptFinalStartGeometry {
    pub(crate) item_id: String,
    pub(crate) scroll_offset: Pixels,
    pub(crate) first_line_height: Pixels,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptFinalStartPlacement {
    pub(crate) item_id: String,
    pub(crate) scroll_offset: Pixels,
    pub(crate) virtual_runway: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptAnchorTheme {
    pub(crate) conversation: TranscriptAnchorRole,
    pub(crate) heading: TranscriptAnchorRole,
    pub(crate) emphasis: TranscriptAnchorRole,
    pub(crate) strong_emphasis: TranscriptAnchorRole,
    pub(crate) code: TranscriptAnchorRole,
    pub(crate) code_panel: TranscriptAnchorRole,
    pub(crate) code_panel_header: TranscriptAnchorRole,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptAnchorRole {
    pub(crate) font_family: String,
    pub(crate) font_size: f32,
    pub(crate) font_weight: u16,
}

impl TranscriptSubmitAnchor {
    pub(crate) fn new(
        turn_index: usize,
        row_identity: Option<String>,
        fragment_index: usize,
        user_input: String,
    ) -> Self {
        Self {
            turn_index,
            row_identity,
            fragment_index,
            user_input,
        }
    }

    pub(crate) fn snapshot(
        &self,
        viewport_action: TranscriptSubmitViewportAction,
    ) -> TranscriptSubmitAnchorSnapshot {
        TranscriptSubmitAnchorSnapshot {
            turn_index: self.turn_index,
            row_identity: self.row_identity.clone(),
            fragment_index: self.fragment_index,
            user_input: self.user_input.clone(),
            viewport_action,
        }
    }

    pub(crate) fn turn_index(&self) -> usize {
        self.turn_index
    }

    pub(crate) fn row_identity(&self) -> Option<&str> {
        self.row_identity.as_deref()
    }

    pub(crate) fn shift_turn_index(&mut self, amount: usize) {
        self.turn_index = self.turn_index.saturating_add(amount);
    }
}

pub(crate) fn prompt_viewport_placement(
    snapshot: &TranscriptSubmitAnchorSnapshot,
    preceding_prompt_plans: &[&BlockRenderPlan],
    prompt_plan: &BlockRenderPlan,
    transcript_width: Pixels,
    viewport_height: Pixels,
    measured_row_height: Option<Pixels>,
    theme: &TranscriptAnchorTheme,
    transcript_code_columns: usize,
    window: &mut Window,
) -> TranscriptPromptViewportPlacement {
    let geometry = prompt_fragment_geometry(
        snapshot,
        preceding_prompt_plans,
        prompt_plan,
        transcript_width,
        theme,
        transcript_code_columns,
        window,
    );
    prompt_viewport_placement_from_geometry(geometry, viewport_height, measured_row_height)
}

pub(crate) fn prompt_fragment_geometry(
    snapshot: &TranscriptSubmitAnchorSnapshot,
    preceding_prompt_plans: &[&BlockRenderPlan],
    prompt_plan: &BlockRenderPlan,
    transcript_width: Pixels,
    theme: &TranscriptAnchorTheme,
    transcript_code_columns: usize,
    window: &mut Window,
) -> TranscriptPromptFragmentGeometry {
    let prompt_width = prompt_text_width(transcript_width);
    let mut measurer = WindowPromptMeasurer::new(theme, window);
    let layout = prompt_markdown_layout_from_plan(
        prompt_plan,
        prompt_width,
        transcript_code_columns,
        &mut measurer,
    );
    let conversation_line_height =
        measurer.block_line_height(markdown_layout::AnchorBlockRole::Conversation);

    let preceding_layouts = preceding_prompt_plans
        .iter()
        .map(|plan| {
            prompt_markdown_layout_from_plan(
                plan,
                prompt_width,
                transcript_code_columns,
                &mut measurer,
            )
        })
        .collect::<Vec<_>>();

    prompt_fragment_geometry_from_layouts(
        snapshot.turn_index,
        preceding_layouts.as_slice(),
        &layout,
        conversation_line_height,
    )
}

#[allow(dead_code)]
pub(crate) fn prompt_last_line_top_offset(
    snapshot: &TranscriptSubmitAnchorSnapshot,
    preceding_prompt_plans: &[&BlockRenderPlan],
    prompt_plan: &BlockRenderPlan,
    transcript_width: Pixels,
    theme: &TranscriptAnchorTheme,
    transcript_code_columns: usize,
    window: &mut Window,
) -> Pixels {
    prompt_fragment_geometry(
        snapshot,
        preceding_prompt_plans,
        prompt_plan,
        transcript_width,
        theme,
        transcript_code_columns,
        window,
    )
    .last_visual_line_top_offset
}

#[allow(dead_code)]
pub(crate) fn commentary_follow_geometry(
    item_id: Option<&str>,
    items: &[TranscriptNarrativeItemGeometry],
    viewport_height: Pixels,
) -> Option<TranscriptCommentaryFollowGeometry> {
    let item = narrative_item_geometry(item_id, items)?;
    let item_bottom_offset = item.bottom_offset();
    Some(TranscriptCommentaryFollowGeometry {
        item_id: item.item_id.clone(),
        item_bottom_offset,
        scroll_offset: (item_bottom_offset - viewport_height.max(px(0.0))).max(px(0.0)),
    })
}

#[allow(dead_code)]
pub(crate) fn final_answer_start_geometry(
    item_id: &str,
    items: &[TranscriptNarrativeItemGeometry],
) -> Option<TranscriptFinalStartGeometry> {
    let item = narrative_item_geometry(Some(item_id), items)?;
    Some(TranscriptFinalStartGeometry {
        item_id: item_id.to_string(),
        scroll_offset: item.top_offset.max(px(0.0)),
        first_line_height: item.first_line_height,
    })
}

#[allow(dead_code)]
pub(crate) fn final_start_top_paint_guard(final_line_height: Pixels) -> Pixels {
    (final_line_height.max(px(0.0)) * FINAL_START_LINE_HEIGHT_GUARD_RATIO)
        .max(px(FINAL_START_MIN_TOP_GUARD))
}

#[allow(dead_code)]
pub(crate) fn final_answer_start_placement(
    item_id: &str,
    items: &[TranscriptNarrativeItemGeometry],
    viewport_height: Pixels,
    measured_row_height: Option<Pixels>,
) -> Option<TranscriptFinalStartPlacement> {
    let geometry = final_answer_start_geometry(item_id, items)?;
    let scroll_offset = (geometry.scroll_offset
        - final_start_top_paint_guard(geometry.first_line_height))
    .max(px(0.0));
    let rendered_bottom = items
        .iter()
        .map(TranscriptNarrativeItemGeometry::bottom_offset)
        .fold(px(0.0), Pixels::max);
    let estimated_row_height = measured_row_height
        .unwrap_or(rendered_bottom)
        .max(rendered_bottom);
    let content_below_anchor = (estimated_row_height - scroll_offset).max(px(0.0));

    Some(TranscriptFinalStartPlacement {
        item_id: geometry.item_id,
        scroll_offset,
        virtual_runway: trailing_scroll_slack(viewport_height, Some(content_below_anchor)),
    })
}

pub(crate) fn narrative_item_geometries(
    turn_index: usize,
    blocks: &[TranscriptNarrativeBlockPlan],
    transcript_width: Pixels,
    themes: TranscriptNarrativeAnchorThemes<'_>,
    transcript_code_columns: usize,
    window: &mut Window,
) -> Vec<TranscriptNarrativeItemGeometry> {
    let prompt_width = prompt_text_width(transcript_width);
    let assistant_width = assistant_narrative_text_width(transcript_width);
    let mut cursor = prompt_fragment_start_offset(turn_index);
    let mut rendered_block_count = 0usize;
    let mut geometries = Vec::new();

    for block in blocks {
        if rendered_block_count > 0 {
            cursor += px(TURN_CARD_BLOCK_GAP);
        }
        let (item_id, height, first_line_height) = match block {
            TranscriptNarrativeBlockPlan::UserPrompt { plan } => {
                let mut measurer = WindowPromptMeasurer::new(themes.prompt, window);
                let layout = prompt_markdown_layout_from_plan(
                    plan,
                    prompt_width,
                    transcript_code_columns,
                    &mut measurer,
                );
                (
                    None,
                    prompt_block_outer_height(layout.height),
                    layout.first_line_height,
                )
            }
            TranscriptNarrativeBlockPlan::AssistantMarkdown {
                item_id,
                plan,
                role,
            } => {
                let mut measurer = WindowPromptMeasurer::new(themes.theme_for(*role), window);
                let layout = prompt_markdown_layout_from_plan(
                    plan,
                    assistant_width,
                    transcript_code_columns,
                    &mut measurer,
                );
                (
                    Some(item_id.clone()),
                    layout.height,
                    layout.first_line_height,
                )
            }
            TranscriptNarrativeBlockPlan::Anonymous { height } => {
                let height = (*height).max(px(0.0));
                (None, height, height)
            }
        };
        geometries.push(TranscriptNarrativeItemGeometry::new(
            item_id,
            cursor,
            height,
            first_line_height,
        ));
        cursor += height;
        rendered_block_count = rendered_block_count.saturating_add(1);
    }

    geometries
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

impl TranscriptPromptFragmentGeometry {
    fn tail_line_top_for_minimum(&self, minimum_top: Pixels) -> Pixels {
        self.visual_line_top_offsets
            .iter()
            .copied()
            .find(|line_top| *line_top >= minimum_top)
            .unwrap_or(minimum_top)
            .max(self.fragment_start_offset)
            .min(self.fragment_tail_offset)
    }
}

impl TranscriptNarrativeItemGeometry {
    #[allow(dead_code)]
    pub(crate) fn new(
        item_id: Option<String>,
        top_offset: Pixels,
        height: Pixels,
        first_line_height: Pixels,
    ) -> Self {
        Self {
            item_id,
            top_offset,
            height,
            first_line_height,
        }
    }

    fn bottom_offset(&self) -> Pixels {
        self.top_offset + self.height.max(px(0.0))
    }
}

impl TranscriptNarrativeAnchorThemes<'_> {
    fn theme_for(&self, role: TranscriptNarrativeTextRole) -> &TranscriptAnchorTheme {
        match role {
            TranscriptNarrativeTextRole::AssistantFinal => self.assistant_final,
            TranscriptNarrativeTextRole::AssistantCommentary => self.assistant_commentary,
            TranscriptNarrativeTextRole::AssistantReasoning => self.assistant_reasoning,
        }
    }
}

fn prompt_fragment_geometry_from_layouts(
    turn_index: usize,
    preceding_layouts: &[markdown_layout::PromptBlockLayout],
    prompt_layout: &markdown_layout::PromptBlockLayout,
    conversation_line_height: Pixels,
) -> TranscriptPromptFragmentGeometry {
    let preceding_height = preceding_layouts
        .iter()
        .map(|layout| prompt_block_outer_height(layout.height) + px(TURN_CARD_BLOCK_GAP))
        .fold(px(0.0), |total, height| total + height);
    let fragment_start_offset = prompt_fragment_start_offset(turn_index) + preceding_height;
    let fragment_content_start_offset = fragment_start_offset + prompt_block_content_top_offset();
    let fragment_tail_offset =
        fragment_start_offset + prompt_block_outer_height(prompt_layout.height);
    let visual_line_top_offsets = prompt_layout
        .visual_line_tops
        .iter()
        .map(|line_top| fragment_content_start_offset + *line_top)
        .collect::<Vec<_>>();

    TranscriptPromptFragmentGeometry {
        fragment_start_offset,
        fragment_content_start_offset,
        fragment_tail_offset,
        last_visual_line_top_offset: fragment_content_start_offset + prompt_layout.last_line_top,
        conversation_line_height,
        visual_line_top_offsets,
    }
}

fn prompt_viewport_placement_from_geometry(
    geometry: TranscriptPromptFragmentGeometry,
    viewport_height: Pixels,
    measured_row_height: Option<Pixels>,
) -> TranscriptPromptViewportPlacement {
    let prompt_outer_height = geometry.fragment_tail_offset - geometry.fragment_start_offset;
    let desired_runway =
        reserved_response_runway(viewport_height, geometry.conversation_line_height);
    let prompt_area_with_runway = (viewport_height - desired_runway).max(px(0.0));
    let prompt_fits_viewport = prompt_outer_height <= prompt_area_with_runway;
    let (scroll_offset, anchor_kind) = if prompt_fits_viewport {
        (
            geometry.fragment_start_offset,
            TranscriptPromptAnchorKind::FragmentStart,
        )
    } else {
        let prompt_area = (viewport_height - desired_runway).max(px(0.0));
        let minimum_top = (geometry.fragment_tail_offset - prompt_area)
            .max(geometry.fragment_start_offset)
            .min(geometry.fragment_tail_offset);
        (
            geometry.tail_line_top_for_minimum(minimum_top),
            TranscriptPromptAnchorKind::FragmentTail,
        )
    };
    let estimated_row_height = measured_row_height.unwrap_or(geometry.fragment_tail_offset);
    let measured_content_below_anchor = Some((estimated_row_height - scroll_offset).max(px(0.0)));
    let virtual_runway = trailing_scroll_slack(viewport_height, measured_content_below_anchor);

    TranscriptPromptViewportPlacement {
        scroll_offset,
        virtual_runway,
        anchor_kind,
        prompt: geometry,
    }
}

fn reserved_response_runway(viewport_height: Pixels, line_height: Pixels) -> Pixels {
    let viewport_height = viewport_height.max(px(0.0));
    let line_height = line_height.max(px(1.0));
    let desired = line_height * RESPONSE_RUNWAY_LINES;
    let minimum_prompt_area = line_height * MIN_VISIBLE_PROMPT_LINES_WITH_RUNWAY;

    if viewport_height <= minimum_prompt_area {
        return px(0.0);
    }

    desired.min(viewport_height - minimum_prompt_area)
}

fn narrative_item_geometry<'a>(
    item_id: Option<&str>,
    items: &'a [TranscriptNarrativeItemGeometry],
) -> Option<&'a TranscriptNarrativeItemGeometry> {
    match item_id {
        Some(item_id) => items
            .iter()
            .find(|item| item.item_id.as_deref() == Some(item_id)),
        None => items.last(),
    }
}

fn prompt_text_width(transcript_width: Pixels) -> Pixels {
    (transcript_width - px(USER_PROMPT_HORIZONTAL_CHROME)).max(px(1.0))
}

fn assistant_narrative_text_width(transcript_width: Pixels) -> Pixels {
    (transcript_width - px(TURN_ROW_HORIZONTAL_PADDING)).max(px(1.0))
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

#[allow(dead_code)]
fn prompt_content_top_offset(turn_index: usize) -> Pixels {
    prompt_fragment_start_offset(turn_index) + prompt_block_content_top_offset()
}

fn prompt_fragment_start_offset(turn_index: usize) -> Pixels {
    let first_turn_top_padding = if turn_index == 0 {
        px(FIRST_TURN_TOP_PADDING)
    } else {
        px(0.0)
    };

    first_turn_top_padding
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
    use gpui::{Pixels, px};

    use super::super::transcript_markdown::InlineRenderLine;
    use super::markdown_layout::{AnchorBlockRole, PromptTextMeasurer, prompt_markdown_layout};
    use super::prompt_content_top_offset;
    use super::{TranscriptPromptFragmentGeometry, TranscriptPromptViewportPlacement};

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

    pub(crate) fn prompt_text_width(transcript_width: Pixels) -> Pixels {
        super::prompt_text_width(transcript_width)
    }

    pub(crate) fn assistant_narrative_text_width(transcript_width: Pixels) -> Pixels {
        super::assistant_narrative_text_width(transcript_width)
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
            conversation_m_advance: px(8.0),
        };
        prompt_content_top_offset(turn_index)
            + prompt_markdown_layout(source, prompt_width, transcript_code_columns, &mut measurer)
                .last_line_top
    }

    pub(crate) fn prompt_geometry_from_markdown_no_wrap(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
    ) -> TranscriptPromptFragmentGeometry {
        let mut measurer = FixedPromptMeasurer {
            line_height,
            heading_line_height,
            code_line_height,
            code_header_line_height,
            inline_columns: None,
            conversation_m_advance: px(8.0),
        };
        let layout =
            prompt_markdown_layout(source, prompt_width, transcript_code_columns, &mut measurer);
        super::prompt_fragment_geometry_from_layouts(turn_index, &[], &layout, line_height)
    }

    pub(crate) fn prompt_viewport_placement_from_markdown_no_wrap(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
        viewport_height: Pixels,
        measured_row_height: Option<Pixels>,
    ) -> TranscriptPromptViewportPlacement {
        let geometry = prompt_geometry_from_markdown_no_wrap(
            turn_index,
            source,
            prompt_width,
            transcript_code_columns,
            line_height,
            heading_line_height,
            code_line_height,
            code_header_line_height,
        );
        super::prompt_viewport_placement_from_geometry(
            geometry,
            viewport_height,
            measured_row_height,
        )
    }

    pub(crate) fn prompt_last_line_top_offset_from_markdown_columns(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        inline_columns: usize,
        _code_columns: usize,
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
            conversation_m_advance: px(8.0),
        };
        prompt_content_top_offset(turn_index)
            + prompt_markdown_layout(source, prompt_width, transcript_code_columns, &mut measurer)
                .last_line_top
    }

    pub(crate) fn prompt_last_line_top_offset_from_markdown_char_width(
        turn_index: usize,
        source: &str,
        prompt_width: Pixels,
        transcript_code_columns: usize,
        inline_char_width: Pixels,
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
    ) -> Pixels {
        let mut measurer = CharWidthPromptMeasurer {
            line_height,
            heading_line_height,
            code_line_height,
            code_header_line_height,
            inline_char_width,
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
        conversation_m_advance: Pixels,
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

        fn conversation_m_advance(&mut self) -> Pixels {
            self.conversation_m_advance
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
    }

    struct CharWidthPromptMeasurer {
        line_height: Pixels,
        heading_line_height: Pixels,
        code_line_height: Pixels,
        code_header_line_height: Pixels,
        inline_char_width: Pixels,
    }

    impl PromptTextMeasurer for CharWidthPromptMeasurer {
        fn inline_visual_line_count(
            &mut self,
            line: &InlineRenderLine,
            _role: AnchorBlockRole,
            wrap_width: Pixels,
        ) -> usize {
            let columns = ((wrap_width / self.inline_char_width).floor() as usize).max(1);
            let len = line
                .fragments
                .iter()
                .map(|fragment| fragment.text.chars().count())
                .sum::<usize>();
            len.max(1).div_ceil(columns)
        }

        fn conversation_m_advance(&mut self) -> Pixels {
            self.inline_char_width
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
    }
}
