use beryl_state::{
    ResolvedAppearance, ThemeFontStyle, ThemePropertyId as Property, ThemeTextDecoration,
    ThemeValue, builtin_fallback_appearance,
};
use gpui::{
    Font, FontStyle, FontWeight, Rgba, SharedString, StrikethroughStyle, TextRun, font, px, rgb,
};

#[derive(Clone, Copy)]
pub(super) enum NarrativeBlockStyle {
    Paragraph,
    Heading(u8),
    Quote,
    List,
    ThematicBreak,
}

#[cfg(test)]
mod tests {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/syndic_transcript_panel/narrative_appearance.rs"
    ));
}

#[derive(Clone, Copy, Default)]
pub(super) struct NarrativeInlineStyle {
    pub(super) emphasis: bool,
    pub(super) strong: bool,
    pub(super) strikethrough: bool,
    pub(super) code: bool,
    pub(super) link_label: bool,
}

#[derive(Clone)]
pub(super) struct NarrativeTextAppearance {
    pub(super) font: Font,
    pub(super) size: f32,
    pub(super) color: Rgba,
    pub(super) strikethrough: bool,
}

impl NarrativeTextAppearance {
    pub(super) fn run(&self, len: usize) -> TextRun {
        TextRun {
            len,
            font: self.font.clone(),
            color: self.color.into(),
            background_color: None,
            underline: None,
            strikethrough: self.strikethrough.then(|| StrikethroughStyle {
                thickness: px((self.size / 14.0).max(1.0)),
                color: Some(self.color.into()),
            }),
        }
    }
}

#[derive(Clone)]
pub(super) struct NarrativeBlockAppearance {
    pub(super) text: NarrativeTextAppearance,
    pub(super) indent: f32,
    pub(super) border_width: f32,
    pub(super) border: Rgba,
}

#[derive(Clone)]
pub(super) struct NarrativeAppearance {
    blocks: [NarrativeBlockAppearance; 10],
    emphasis: FontStyle,
    strong: FontWeight,
    strikethrough: bool,
    code_family: SharedString,
    link_color: Rgba,
    pub(super) marker: NarrativeMarkerAppearance,
}

#[derive(Clone, Copy)]
pub(super) struct NarrativeMarkerAppearance {
    pub(super) foreground: Rgba,
    pub(super) background: Rgba,
    pub(super) selected_foreground: Rgba,
    pub(super) selected_background: Rgba,
    pub(super) hover_background: Rgba,
    pub(super) hover_foreground: Rgba,
    pub(super) focus: Rgba,
}

impl Default for NarrativeAppearance {
    fn default() -> Self {
        Self::prepare(&builtin_fallback_appearance())
    }
}

impl NarrativeAppearance {
    pub(super) fn prepare(appearance: &ResolvedAppearance) -> Self {
        let property = |role, property| {
            appearance
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .expect("canonical narrative appearance property")
        };
        let color = |role, kind| {
            let ThemeValue::Color(value) = property(role, kind) else {
                unreachable!("canonical narrative color")
            };
            let [r, g, b] = value.rgb();
            rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        };
        let pixels = |role, kind| {
            let ThemeValue::LogicalPixels(value) = property(role, kind) else {
                unreachable!("canonical narrative logical pixels")
            };
            value.get()
        };
        let weight = |role| {
            let ThemeValue::FontWeight(value) = property(role, Property::FontWeight) else {
                unreachable!("canonical narrative font weight")
            };
            FontWeight(f32::from(value.get()))
        };
        let family = |role| {
            let ThemeValue::FontFamily(value) = property(role, Property::FontFamily) else {
                unreachable!("canonical narrative font family")
            };
            SharedString::from(value.as_str().to_owned())
        };
        let body_font = font(family("transcript.turn.assistant.final"));
        let body_size = pixels("transcript.turn.assistant.final", Property::FontSize);
        let body_color = color("transcript-view.narrative-text", Property::Foreground);
        let blocks = [
            "transcript-view.narrative-block",
            "transcript-view.narrative-block.heading-1",
            "transcript-view.narrative-block.heading-2",
            "transcript-view.narrative-block.heading-3",
            "transcript-view.narrative-block.heading-4",
            "transcript-view.narrative-block.heading-5",
            "transcript-view.narrative-block.heading-6",
            "transcript-view.narrative-block.quote",
            "transcript-view.narrative-block.list",
            "transcript-view.narrative-block.thematic-break",
        ]
        .map(|role| {
            let ThemeValue::FontScale(scale) = property(role, Property::FontScale) else {
                unreachable!("canonical narrative font scale")
            };
            let mut font = body_font.clone();
            font.weight = weight(role);
            NarrativeBlockAppearance {
                text: NarrativeTextAppearance {
                    font,
                    size: body_size * scale.get(),
                    color: body_color,
                    strikethrough: false,
                },
                indent: pixels(role, Property::Indent) * (body_size / 14.0),
                border_width: pixels(role, Property::BorderWidth),
                border: color(role, Property::BorderColor),
            }
        });
        let ThemeValue::FontStyle(emphasis) = property(
            "transcript-view.narrative-text.emphasis",
            Property::FontStyle,
        ) else {
            unreachable!("canonical narrative emphasis")
        };
        let ThemeValue::TextDecoration(strike) = property(
            "transcript-view.narrative-text.strike",
            Property::TextDecoration,
        ) else {
            unreachable!("canonical narrative strike")
        };
        Self {
            blocks,
            marker: NarrativeMarkerAppearance {
                foreground: color("transcript.image_marker", Property::Foreground),
                background: color("transcript.image_marker", Property::TextBackground),
                selected_foreground: color("transcript.selection", Property::Foreground),
                selected_background: color("transcript.selection", Property::TextBackground),
                hover_background: color("interaction.hover", Property::Background),
                hover_foreground: color("interaction.hover", Property::Foreground),
                focus: color("accent_marker", Property::Color),
            },
            emphasis: match emphasis {
                ThemeFontStyle::Normal => FontStyle::Normal,
                ThemeFontStyle::Italic => FontStyle::Italic,
            },
            strong: weight("transcript-view.narrative-text.strong"),
            strikethrough: *strike == ThemeTextDecoration::LineThrough,
            code_family: family("transcript-view.narrative-text.inline-code"),
            link_color: color(
                "transcript-view.narrative-text.link-label",
                Property::Foreground,
            ),
        }
    }

    pub(super) fn block(&self, style: NarrativeBlockStyle) -> NarrativeBlockAppearance {
        let index = match style {
            NarrativeBlockStyle::Paragraph => 0,
            NarrativeBlockStyle::Heading(level @ 1..=6) => usize::from(level),
            NarrativeBlockStyle::Heading(_) => 0,
            NarrativeBlockStyle::Quote => 7,
            NarrativeBlockStyle::List => 8,
            NarrativeBlockStyle::ThematicBreak => 9,
        };
        self.blocks[index].clone()
    }

    pub(super) fn inline(
        &self,
        block: &NarrativeBlockAppearance,
        style: NarrativeInlineStyle,
    ) -> NarrativeTextAppearance {
        let mut text = block.text.clone();
        if style.emphasis {
            text.font.style = self.emphasis;
        }
        if style.strong {
            text.font.weight = self.strong;
        }
        if style.code {
            text.font.family = self.code_family.clone();
        }
        if style.link_label {
            text.color = self.link_color;
        }
        if style.strikethrough {
            text.strikethrough = self.strikethrough;
        }
        text
    }
}
