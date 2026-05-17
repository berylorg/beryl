use std::ops::Deref;

use super::{ChromeButtonTheme, ShellRenderStyleSnapshot};
use crate::shell::ShellView;

pub(in crate::shell) struct ShellRenderFrame<'a> {
    shell: &'a ShellView,
    style: ShellRenderStyleSnapshot,
}

impl<'a> ShellRenderFrame<'a> {
    pub(in crate::shell) fn new(shell: &'a ShellView, style: ShellRenderStyleSnapshot) -> Self {
        Self { shell, style }
    }

    pub(in crate::shell) fn style(&self) -> &ShellRenderStyleSnapshot {
        &self.style
    }

    pub(in crate::shell) fn role_background(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.style.role_background(role, fallback)
    }

    pub(in crate::shell) fn role_border(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.style.role_border(role, fallback)
    }

    pub(in crate::shell) fn role_foreground(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::Rgba,
    ) -> gpui::Rgba {
        self.style.role_foreground(role, fallback)
    }

    pub(in crate::shell) fn role_font_family(
        &self,
        role: crate::BerylThemeRole,
        fallback: &'static str,
    ) -> String {
        self.style.role_font_family(role, fallback)
    }

    pub(in crate::shell) fn role_font_weight(
        &self,
        role: crate::BerylThemeRole,
        fallback: gpui::FontWeight,
    ) -> gpui::FontWeight {
        self.style.role_font_weight(role, fallback)
    }
}

impl Deref for ShellRenderFrame<'_> {
    type Target = ShellView;

    fn deref(&self) -> &Self::Target {
        self.shell
    }
}

macro_rules! delegate_frame_style {
    ($($method:ident -> $ty:ty;)+) => {
        impl ShellRenderFrame<'_> {
            $(
                pub(in crate::shell) fn $method(&self) -> $ty {
                    self.style.$method()
                }
            )+
        }
    };
}

delegate_frame_style! {
    general_ui_background -> gpui::Rgba;
    general_ui_foreground -> gpui::Rgba;
    toolbar_background -> gpui::Rgba;
    conversation_thread_strip_background -> gpui::Rgba;
    separator_color -> gpui::Rgba;
    primary_button_theme -> ChromeButtonTheme;
    secondary_button_theme -> ChromeButtonTheme;
    input_panel_background -> gpui::Rgba;
    input_background -> gpui::Rgba;
    input_border -> gpui::Rgba;
    input_foreground -> gpui::Rgba;
    status_line_background -> gpui::Rgba;
    status_line_title_foreground -> gpui::Rgba;
    status_line_value_foreground -> gpui::Rgba;
    panel_surface_background -> gpui::Rgba;
    row_surface_background -> gpui::Rgba;
    popup_surface_background -> gpui::Rgba;
    surface_border -> gpui::Rgba;
    surface_muted_foreground -> gpui::Rgba;
    surface_foreground -> gpui::Rgba;
}
