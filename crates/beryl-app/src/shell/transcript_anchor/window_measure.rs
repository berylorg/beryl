use gpui::{Font, FontStyle, FontWeight, Pixels, SharedString, TextRun, Window, px, rems};

use crate::{AppearanceRoleSettings, AppearanceSettings};

use super::super::transcript_markdown::{InlineRenderLine, InlineRenderRole};
use super::markdown_layout::{AnchorBlockRole, PromptTextMeasurer};
use super::{CODE_FONT_FAMILY, CODE_FONT_SIZE_REM, CODE_HEADER_FONT_SIZE_REM};

pub(super) struct WindowPromptMeasurer<'a, 'w> {
    appearance: &'a AppearanceSettings,
    window: &'w mut Window,
}

impl<'a, 'w> WindowPromptMeasurer<'a, 'w> {
    pub(super) fn new(appearance: &'a AppearanceSettings, window: &'w mut Window) -> Self {
        Self { appearance, window }
    }

    fn role_settings(
        &self,
        role: InlineRenderRole,
        block_role: AnchorBlockRole,
    ) -> &AppearanceRoleSettings {
        match role {
            InlineRenderRole::Conversation => self.block_role_settings(block_role),
            InlineRenderRole::Emphasis => &self.appearance.emphasis,
            InlineRenderRole::StrongEmphasis => &self.appearance.strong_emphasis,
            InlineRenderRole::Code => &self.appearance.code,
        }
    }

    fn block_role_settings(&self, block_role: AnchorBlockRole) -> &AppearanceRoleSettings {
        match block_role {
            AnchorBlockRole::Conversation => &self.appearance.conversation_text,
            AnchorBlockRole::Heading { .. } => &self.appearance.markdown_header,
        }
    }

    fn font_for_role(&self, role: InlineRenderRole, block_role: AnchorBlockRole) -> Font {
        let settings = self.role_settings(role, block_role);
        Font {
            family: SharedString::from(settings.font_family.clone()),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(settings.font_weight as f32),
            style: FontStyle::Normal,
        }
    }

    fn text_style_for_block_role(&self, block_role: AnchorBlockRole) -> gpui::TextStyle {
        let settings = self.block_role_settings(block_role);
        let mut style = self.window.text_style();
        style.font_family = SharedString::from(settings.font_family.clone());
        style.font_weight = FontWeight(settings.font_weight as f32);
        style.font_size = px(block_font_size(settings.font_size, block_role)).into();
        style
    }

    fn text_style_for_rem_font_size(&self, rem_size: f32) -> gpui::TextStyle {
        let mut style = self.window.text_style();
        style.font_size = rems(rem_size).into();
        style
    }
}

impl PromptTextMeasurer for WindowPromptMeasurer<'_, '_> {
    fn inline_visual_line_count(
        &mut self,
        line: &InlineRenderLine,
        role: AnchorBlockRole,
        wrap_width: Pixels,
    ) -> usize {
        let style = self.text_style_for_block_role(role);
        let font_size = style.font_size.to_pixels(self.window.rem_size());

        let (text, runs) = if line.fragments.is_empty() {
            (" ".to_string(), vec![style.to_run(" ".len())])
        } else {
            let mut text = String::new();
            let mut runs = Vec::with_capacity(line.fragments.len());
            for fragment in &line.fragments {
                text.push_str(fragment.text.as_str());
                runs.push(TextRun {
                    len: fragment.text.len(),
                    font: self.font_for_role(fragment.style.role, role),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
            }
            (text, runs)
        };

        self.window
            .text_system()
            .shape_text(
                text.into(),
                font_size,
                runs.as_slice(),
                Some(wrap_width.max(px(1.0))),
                None,
            )
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.wrap_boundaries().len() + 1)
                    .sum::<usize>()
                    .max(1)
            })
            .unwrap_or(1)
    }

    fn block_line_height(&self, role: AnchorBlockRole) -> Pixels {
        self.text_style_for_block_role(role)
            .line_height_in_pixels(self.window.rem_size())
    }

    fn code_line_height(&self) -> Pixels {
        self.text_style_for_rem_font_size(CODE_FONT_SIZE_REM)
            .line_height_in_pixels(self.window.rem_size())
    }

    fn code_header_line_height(&self) -> Pixels {
        self.text_style_for_rem_font_size(CODE_HEADER_FONT_SIZE_REM)
            .line_height_in_pixels(self.window.rem_size())
    }

    fn code_columns_for_width(&mut self, wrap_width: Pixels) -> usize {
        let mut font = self.window.text_style().font();
        font.family = CODE_FONT_FAMILY.into();
        let run = TextRun {
            len: 1,
            font,
            color: self.window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let char_width = self
            .window
            .text_system()
            .shape_line(
                "0".into(),
                rems(CODE_FONT_SIZE_REM).to_pixels(self.window.rem_size()),
                &[run],
                None,
            )
            .width
            .max(px(1.0));

        ((wrap_width.max(px(1.0)) / char_width).floor() as usize).max(1)
    }
}

fn block_font_size(base_size: f32, block_role: AnchorBlockRole) -> f32 {
    match block_role {
        AnchorBlockRole::Conversation => base_size,
        AnchorBlockRole::Heading { level } => match level {
            1 => base_size + 4.0,
            2 => base_size + 2.0,
            3 => base_size + 1.0,
            _ => base_size,
        },
    }
}
