use super::*;
use beryl_state::{ThemePropertyId, ThemeValue};

impl ThreadLineage {
    pub fn set_appearance(
        &mut self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .appearance
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &appearance))
        {
            self.appearance = Some(appearance);
            cx.notify();
        }
    }
    pub fn set_inert(&mut self, inert: bool, cx: &mut Context<Self>) {
        self.set_inert_reason(
            inert.then(|| "Checking this thread's current ancestry.".into()),
            cx,
        );
    }
    pub fn set_inert_reason(&mut self, reason: Option<String>, cx: &mut Context<Self>) {
        let reason = reason.map(|reason| reason.chars().take(512).collect::<String>());
        if self.inert_reason != reason {
            self.inert = reason.is_some();
            self.inert_reason = reason;
            cx.notify();
        }
    }

    pub(super) fn stride(&self) -> f32 {
        self.breadcrumb_width() + 6. + 12. + 6.
    }
    pub(super) fn breadcrumb_width(&self) -> f32 {
        176.
    }
    pub(super) fn current_width(&self) -> f32 {
        220.
    }
    pub(super) fn color(&self, role: &str, property: ThemePropertyId, fallback: u32) -> gpui::Rgba {
        let value = self.appearance.as_ref().and_then(|generation| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
        });
        if let Some(ThemeValue::Color(color)) = value {
            let [r, g, b] = color.rgb();
            gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        } else {
            gpui::rgb(fallback)
        }
    }
    pub(super) fn font(
        &self,
        role: &str,
        size: f32,
        weight: f32,
    ) -> (f32, gpui::FontWeight, Option<gpui::SharedString>) {
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
            .unwrap_or(size);
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
                    Some(gpui::SharedString::from(value.as_str().to_owned()))
                } else {
                    None
                }
            });
        (size, gpui::FontWeight(weight), family)
    }
    pub(super) fn scrollbar_style(&self) -> ScrollbarStyle {
        let value = self.appearance.as_ref().and_then(|generation| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == "scrollbar.thumb")
                .and_then(|(_, style)| style.property(ThemePropertyId::Background))
        });
        let thumb_color = if let Some(ThemeValue::Color(color)) = value {
            let [r, g, b] = color.rgb();
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
        } else {
            0x94a3b8
        };
        ScrollbarStyle {
            thumb_color,
            hit_lane_thickness: px(10.),
            geometry: gpui_scrollbar::ScrollbarGeometryStyle {
                track_inset: px(2.),
                ..Default::default()
            },
            ..ScrollbarStyle::default()
        }
    }
}
