mod button;
mod frame;
mod role_style;

use std::{collections::HashMap, sync::Arc};

use gpui::rgb;

use super::render;
use button::button_theme_from_styles;
pub(super) use button::{ChromeButtonStateTheme, ChromeButtonTheme};
pub(super) use frame::ShellRenderFrame;
use role_style::{
    ShellRoleStyle, shell_role_styles, style_background, style_border, style_foreground,
    style_single_color, style_single_color_packed_rgb,
};

pub(super) struct ShellRenderThemeCache {
    pub(super) projection: crate::ActiveThemeProjection,
    style_snapshot: ShellRenderStyleSnapshot,
}

#[derive(Clone)]
pub(super) struct ShellRenderStyleSnapshot {
    revision: u64,
    role_styles: Arc<HashMap<crate::BerylThemeRole, ShellRoleStyle>>,
    transcript_theme: Arc<render::transcript::TranscriptTheme>,
    general_ui_background: gpui::Rgba,
    general_ui_foreground: gpui::Rgba,
    toolbar_background: gpui::Rgba,
    conversation_thread_strip_background: gpui::Rgba,
    separator_color: gpui::Rgba,
    primary_button_theme: ChromeButtonTheme,
    secondary_button_theme: ChromeButtonTheme,
    input_panel_background: gpui::Rgba,
    input_background: gpui::Rgba,
    input_border: gpui::Rgba,
    input_foreground: gpui::Rgba,
    transcript_shell_background: gpui::Rgba,
    transcript_shell_foreground: gpui::Rgba,
    status_line_background: gpui::Rgba,
    status_line_title_foreground: gpui::Rgba,
    status_line_value_foreground: gpui::Rgba,
    panel_surface_background: gpui::Rgba,
    row_surface_background: gpui::Rgba,
    popup_surface_background: gpui::Rgba,
    surface_border: gpui::Rgba,
    surface_muted_foreground: gpui::Rgba,
    surface_foreground: gpui::Rgba,
    scrollbar_thumb_color: u32,
}

impl ShellRenderThemeCache {
    pub(super) fn new(projection: crate::ActiveThemeProjection) -> Self {
        let role_styles = Arc::new(shell_role_styles(&projection));
        let transcript_theme = Arc::new(render::transcript::TranscriptTheme::from_active_theme(
            &projection,
        ));
        let style_snapshot = ShellRenderStyleSnapshot::new(role_styles, transcript_theme);
        Self {
            projection,
            style_snapshot,
        }
    }

    pub(super) fn style_snapshot(&self) -> ShellRenderStyleSnapshot {
        self.style_snapshot.clone()
    }
}

impl ShellRenderStyleSnapshot {
    fn new(
        role_styles: Arc<HashMap<crate::BerylThemeRole, ShellRoleStyle>>,
        transcript_theme: Arc<render::transcript::TranscriptTheme>,
    ) -> Self {
        let primary_button_theme = button_theme_from_styles(
            &role_styles,
            crate::BerylThemeRole::ButtonPrimaryNormal,
            crate::BerylThemeRole::ButtonPrimaryLabel,
            crate::BerylThemeRole::ButtonPrimaryHover,
            crate::BerylThemeRole::ButtonPrimaryActive,
            crate::BerylThemeRole::ButtonPrimaryDisabled,
            ChromeButtonTheme::primary(),
        );
        let secondary_button_theme = button_theme_from_styles(
            &role_styles,
            crate::BerylThemeRole::ButtonSecondaryNormal,
            crate::BerylThemeRole::ButtonSecondaryLabel,
            crate::BerylThemeRole::ButtonSecondaryHover,
            crate::BerylThemeRole::ButtonSecondaryActive,
            crate::BerylThemeRole::ButtonSecondaryDisabled,
            ChromeButtonTheme::secondary(),
        );

        Self {
            revision: transcript_theme.revision(),
            general_ui_background: style_background(
                &role_styles,
                crate::BerylThemeRole::AppWindow,
                rgb(0x020617),
            ),
            general_ui_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::AppWindow,
                rgb(0xe2e8f0),
            ),
            toolbar_background: style_background(
                &role_styles,
                crate::BerylThemeRole::MainToolbar,
                rgb(0x020617),
            ),
            conversation_thread_strip_background: style_background(
                &role_styles,
                crate::BerylThemeRole::MainThreadStrip,
                rgb(0x091220),
            ),
            separator_color: style_single_color(
                &role_styles,
                crate::BerylThemeRole::MainSeparator,
                rgb(0x1e293b),
            ),
            primary_button_theme,
            secondary_button_theme,
            input_panel_background: style_background(
                &role_styles,
                crate::BerylThemeRole::InputPanel,
                rgb(0x020617),
            ),
            input_background: style_background(
                &role_styles,
                crate::BerylThemeRole::InputField,
                rgb(0x0f172a),
            ),
            input_border: style_border(
                &role_styles,
                crate::BerylThemeRole::InputField,
                rgb(0x334155),
            ),
            input_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::InputFieldText,
                rgb(0xe2e8f0),
            ),
            transcript_shell_background: style_background(
                &role_styles,
                crate::BerylThemeRole::TranscriptShell,
                rgb(0x091220),
            ),
            transcript_shell_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::TranscriptShell,
                rgb(0xe2e8f0),
            ),
            status_line_background: style_background(
                &role_styles,
                crate::BerylThemeRole::StatusLine,
                rgb(0x020617),
            ),
            status_line_title_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::StatusLine,
                rgb(0x94a3b8),
            ),
            status_line_value_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::StatusValueOk,
                rgb(0xe2e8f0),
            ),
            panel_surface_background: style_background(
                &role_styles,
                crate::BerylThemeRole::Panel,
                rgb(0x111827),
            ),
            row_surface_background: style_background(
                &role_styles,
                crate::BerylThemeRole::SurfaceRow,
                rgb(0x1f2937),
            ),
            popup_surface_background: style_background(
                &role_styles,
                crate::BerylThemeRole::PopupSurface,
                rgb(0x111827),
            ),
            surface_border: style_border(&role_styles, crate::BerylThemeRole::Panel, rgb(0x374151)),
            surface_muted_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::TextMuted,
                rgb(0x94a3b8),
            ),
            surface_foreground: style_foreground(
                &role_styles,
                crate::BerylThemeRole::Panel,
                rgb(0xe2e8f0),
            ),
            scrollbar_thumb_color: style_single_color_packed_rgb(
                &role_styles,
                crate::BerylThemeRole::ScrollbarThumbNormal,
                0x94a3b8,
            ),
            role_styles,
            transcript_theme,
        }
    }

    pub(super) fn transcript_theme(&self) -> Arc<render::transcript::TranscriptTheme> {
        self.transcript_theme.clone()
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn role_background(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.role_styles
            .get(&role)
            .and_then(|style| style.background)
            .unwrap_or(fallback)
    }

    pub(super) fn role_border(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.role_styles
            .get(&role)
            .and_then(|style| style.border)
            .unwrap_or(fallback)
    }

    pub(super) fn role_foreground(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.role_styles
            .get(&role)
            .and_then(|style| style.foreground)
            .unwrap_or(fallback)
    }

    pub(super) fn role_color(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.role_styles
            .get(&role)
            .and_then(|style| style.color)
            .unwrap_or(fallback)
    }

    pub(super) fn role_font_family(
        &self,
        role: crate::BerylThemeRole,
        fallback: &'static str,
    ) -> String {
        self.role_styles
            .get(&role)
            .and_then(|style| style.font_family.clone())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(super) fn role_font_weight(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::FontWeight,
    ) -> gpui::FontWeight {
        self.role_styles
            .get(&role)
            .and_then(|style| style.font_weight)
            .unwrap_or(fallback)
    }

    pub(super) fn general_ui_background(&self) -> gpui::Rgba {
        self.general_ui_background
    }

    pub(super) fn general_ui_foreground(&self) -> gpui::Rgba {
        self.general_ui_foreground
    }

    pub(super) fn toolbar_background(&self) -> gpui::Rgba {
        self.toolbar_background
    }

    pub(super) fn conversation_thread_strip_background(&self) -> gpui::Rgba {
        self.conversation_thread_strip_background
    }

    pub(super) fn separator_color(&self) -> gpui::Rgba {
        self.separator_color
    }

    pub(super) fn primary_button_theme(&self) -> ChromeButtonTheme {
        self.primary_button_theme
    }

    pub(super) fn secondary_button_theme(&self) -> ChromeButtonTheme {
        self.secondary_button_theme
    }

    pub(super) fn input_panel_background(&self) -> gpui::Rgba {
        self.input_panel_background
    }

    pub(super) fn input_background(&self) -> gpui::Rgba {
        self.input_background
    }

    pub(super) fn input_border(&self) -> gpui::Rgba {
        self.input_border
    }

    pub(super) fn input_foreground(&self) -> gpui::Rgba {
        self.input_foreground
    }

    pub(super) fn transcript_shell_background(&self) -> gpui::Rgba {
        self.transcript_shell_background
    }

    pub(super) fn transcript_shell_foreground(&self) -> gpui::Rgba {
        self.transcript_shell_foreground
    }

    pub(super) fn status_line_background(&self) -> gpui::Rgba {
        self.status_line_background
    }

    pub(super) fn status_line_title_foreground(&self) -> gpui::Rgba {
        self.status_line_title_foreground
    }

    pub(super) fn status_line_value_foreground(&self) -> gpui::Rgba {
        self.status_line_value_foreground
    }

    pub(super) fn panel_surface_background(&self) -> gpui::Rgba {
        self.panel_surface_background
    }

    pub(super) fn row_surface_background(&self) -> gpui::Rgba {
        self.row_surface_background
    }

    pub(super) fn popup_surface_background(&self) -> gpui::Rgba {
        self.popup_surface_background
    }

    pub(super) fn surface_border(&self) -> gpui::Rgba {
        self.surface_border
    }

    pub(super) fn surface_muted_foreground(&self) -> gpui::Rgba {
        self.surface_muted_foreground
    }

    pub(super) fn surface_foreground(&self) -> gpui::Rgba {
        self.surface_foreground
    }

    pub(super) fn scrollbar_thumb_color(&self) -> u32 {
        self.scrollbar_thumb_color
    }
}
