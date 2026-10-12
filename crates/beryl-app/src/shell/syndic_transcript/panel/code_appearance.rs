use beryl_state::{ResolvedAppearance, ThemePropertyId as Property, ThemeValue};
use gpui::{App, Font, FontWeight, Rgba, SharedString, font, px, rgb};
use gpui_scrollbar::ScrollbarStyle;

#[derive(Clone)]
pub(super) struct CodeAppearance {
    pub(super) container: Rgba,
    pub(super) header: Rgba,
    pub(super) header_text: Rgba,
    pub(super) body: Rgba,
    pub(super) body_text: Rgba,
    pub(super) border: Rgba,
    pub(super) selected_border: Rgba,
    pub(super) scrollbar: ScrollbarStyle,
    pub(super) header_font: CodeFont,
    pub(super) body_font: CodeFont,
    pub(super) command_theme: crate::widgets::command_style::ChromeButtonTheme,
    pub(super) command_font: Font,
}

#[derive(Clone)]
pub(super) struct CodeFont {
    pub(super) font: Font,
    pub(super) size: f32,
}

impl Default for CodeAppearance {
    fn default() -> Self {
        Self {
            container: rgb(0xf8fafc),
            header: rgb(0xf8fafc),
            header_text: rgb(0x111827),
            body: rgb(0xf8fafc),
            body_text: rgb(0x111827),
            border: rgb(0xcbd5e1),
            selected_border: rgb(0x38bdf8),
            scrollbar: ScrollbarStyle::default(),
            header_font: CodeFont {
                font: font("Consolas"),
                size: 13.0,
            },
            body_font: CodeFont {
                font: font("Consolas"),
                size: 13.0,
            },
            command_theme: crate::widgets::command_style::ChromeButtonTheme::secondary(),
            command_font: font("Segoe UI"),
        }
    }
}

impl CodeAppearance {
    pub(super) fn header_line_height(&self) -> f32 {
        self.header_font.size * 1.5
    }

    pub(super) fn header_height(&self) -> f32 {
        self.header_line_height()
            .max(crate::widgets::command_style::BUTTON_OUTER_HEIGHT)
            + 1.0
    }

    pub(super) fn line_height(
        &self,
        cx: &App,
    ) -> Result<(f32, gpui::FontEnvironmentKey), super::code_layout::CodeLayoutError> {
        use super::code_layout::{CodeLayoutError, checked_shaping_error};
        let system = cx.text_system();
        let before = system
            .font_environment_key()
            .ok_or(CodeLayoutError::FontEnvironmentUnavailable)?;
        let id = system
            .try_resolve_font(&self.body_font.font)
            .map_err(checked_shaping_error)?;
        let size = px(self.body_font.size);
        let height = (f32::from(system.ascent(id, size)).abs()
            + f32::from(system.descent(id, size)).abs())
        .ceil()
        .max(super::code_viewport::LINE_HEIGHT);
        if system.font_environment_key() != Some(before) {
            return Err(CodeLayoutError::FontEnvironmentChanged);
        }
        if !height.is_finite() {
            return Err(CodeLayoutError::InvalidGeometry);
        }
        Ok((height, before))
    }

    pub(super) fn prepare(appearance: &ResolvedAppearance) -> Self {
        let property = |role, property| {
            appearance
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .expect("canonical Code theme property")
        };
        let typography = |role| {
            let ThemeValue::FontFamily(family) = property(role, Property::FontFamily) else {
                unreachable!("canonical Code font family")
            };
            let ThemeValue::LogicalPixels(size) = property(role, Property::FontSize) else {
                unreachable!("canonical Code font size")
            };
            let ThemeValue::FontWeight(weight) = property(role, Property::FontWeight) else {
                unreachable!("canonical Code font weight")
            };
            let mut font = font(SharedString::from(family.as_str().to_owned()));
            font.weight = FontWeight(f32::from(weight.get()));
            CodeFont {
                font,
                size: size.get(),
            }
        };
        let color = |role, property| {
            let value = appearance
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .expect("canonical Code theme property");
            let ThemeValue::Color(value) = value else {
                unreachable!("canonical Code color property")
            };
            let [r, g, b] = value.rgb();
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
        };
        let command_font = typography("button.secondary.label").font;
        let command_state = |role| crate::widgets::command_style::ChromeButtonStateTheme {
            background: rgb(color(role, Property::Background)),
            border: rgb(color(role, Property::Border)),
            foreground: rgb(color(role, Property::Foreground)),
        };
        let command_theme = crate::widgets::command_style::ChromeButtonTheme {
            font_weight: command_font.weight,
            normal: command_state("button.secondary.normal"),
            hover: command_state("button.secondary.hover"),
            active: command_state("button.secondary.active"),
            disabled: command_state("button.secondary.disabled"),
        };
        Self {
            container: rgb(color("code_panel.container", Property::Background)),
            header: rgb(color("code_panel.header", Property::Background)),
            header_text: rgb(color("code_panel.header.text", Property::Foreground)),
            body: rgb(color("code_panel.body", Property::Background)),
            body_text: rgb(color("code_panel.body.text", Property::Foreground)),
            border: rgb(color("code_panel.border", Property::Color)),
            selected_border: rgb(color("focus.ring", Property::Color)),
            scrollbar: ScrollbarStyle {
                thumb_color: color("scrollbar.thumb.normal", Property::Color),
                ..Default::default()
            },
            header_font: typography("code_panel.header.text"),
            body_font: typography("code_panel.body.text"),
            command_theme,
            command_font,
        }
    }
}
