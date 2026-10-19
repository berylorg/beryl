use super::*;
use beryl_state::{ThemePropertyId, ThemeValue};

impl ActivityPanel {
    pub(crate) fn set_appearance(
        &mut self,
        generation: Arc<crate::theme_runtime::AppearanceGeneration>,
        cx: &mut Context<Self>,
    ) {
        if self
            .appearance
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &generation))
        {
            self.appearance = Some(generation);
            cx.notify();
        }
    }

    pub(super) fn color(&self, role: &str, property: ThemePropertyId, fallback: u32) -> gpui::Rgba {
        let role = match (role, property) {
            ("command-button", ThemePropertyId::Foreground) => "button.secondary.label",
            ("command-button", _) => "button.secondary.normal",
            ("command-button.hover", _) => "button.secondary.hover",
            ("command-button.pressed", _) => "button.secondary.pressed",
            ("command-button.disabled", _) => "button.secondary.disabled",
            ("command-button.focused", _) => "focus.ring",
            ("tooltip", ThemePropertyId::Foreground) => "tooltip.text",
            _ => role,
        };
        let value = self.appearance.as_ref().and_then(|generation| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
        });
        match value {
            Some(ThemeValue::Color(color)) => {
                let [r, g, b] = color.rgb();
                gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
            }
            _ => gpui::rgb(fallback),
        }
    }

    pub(super) fn font(
        &self,
        role: &str,
        fallback: f32,
        weight: f32,
    ) -> (f32, gpui::FontWeight, Option<gpui::SharedString>) {
        let role = match role {
            "command-button" => "button.secondary.label",
            "tooltip" => "tooltip.text",
            _ => role,
        };
        let style = self.appearance.as_ref().and_then(|generation| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .map(|(_, style)| style)
        });
        let size = style
            .and_then(|style| style.property(ThemePropertyId::FontSize))
            .and_then(|value| {
                if let ThemeValue::LogicalPixels(value) = value {
                    Some(value.get())
                } else {
                    None
                }
            })
            .unwrap_or(fallback);
        let weight = style
            .and_then(|style| style.property(ThemePropertyId::FontWeight))
            .and_then(|value| {
                if let ThemeValue::FontWeight(value) = value {
                    Some(value.get() as f32)
                } else {
                    None
                }
            })
            .unwrap_or(weight);
        let family = style
            .and_then(|style| style.property(ThemePropertyId::FontFamily))
            .and_then(|value| {
                if let ThemeValue::FontFamily(value) = value {
                    Some(value.as_str().to_owned().into())
                } else {
                    None
                }
            });
        (size, gpui::FontWeight(weight), family)
    }
}
