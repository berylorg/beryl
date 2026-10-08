use super::ThreadRootPicker;
use beryl_state::{ThemePropertyId, ThemeValue};
use gpui::{Rgba, rgb};

#[derive(Clone, Copy, Debug)]
pub struct ThreadRootPickerStyle {
    pub width: f32,
    pub height: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub gap: f32,
    pub radius: f32,
    pub border_width: f32,
    pub header_height: f32,
    pub header_gap: f32,
    pub search_height: f32,
    pub heading_height: f32,
    pub row_height: f32,
    pub row_gap: f32,
    pub row_padding_x: f32,
    pub row_content_gap: f32,
    pub row_radius: f32,
    pub row_border_width: f32,
    pub row_icon_size: f32,
    pub ring_width: f32,
    pub unavailable_opacity: f32,
    pub root_row_height: f32,
    pub runtime_viewport_height: f32,
    pub runtime_row_height: f32,
    pub runtime_row_gap: f32,
    pub command_height: f32,
    pub footer_height: f32,
}

impl Default for ThreadRootPickerStyle {
    fn default() -> Self {
        Self {
            width: 732.,
            height: 616.,
            padding_x: 26.,
            padding_y: 18.,
            gap: 12.,
            radius: 10.,
            border_width: 1.,
            header_height: 64.,
            header_gap: 4.,
            search_height: 36.,
            heading_height: 16.,
            row_height: 52.,
            row_gap: 6.,
            row_padding_x: 14.,
            row_content_gap: 10.,
            row_radius: 7.,
            row_border_width: 1.,
            row_icon_size: 18.,
            ring_width: 2.,
            unavailable_opacity: 0.72,
            root_row_height: 48.,
            runtime_viewport_height: 102.,
            runtime_row_height: 48.,
            runtime_row_gap: 6.,
            command_height: 32.,
            footer_height: 60.,
        }
    }
}

impl ThreadRootPickerStyle {
    pub fn runtime_row_stride(&self) -> f32 {
        (self.runtime_row_height + self.runtime_row_gap).max(1.)
    }
    pub fn row_stride(&self) -> f32 {
        (self.row_height + self.row_gap).max(1.)
    }
    pub fn viewport_height(&self) -> f32 {
        (self.height
            - self.padding_y * 2.
            - self.border_width * 2.
            - self.header_height
            - self.search_height
            - self.heading_height
            - self.gap * 3.)
            .max(1.)
    }
}

impl ThreadRootPicker {
    pub(super) fn color(&self, role: &str, property: ThemePropertyId, fallback: u32) -> Rgba {
        let value = self.config.appearance.as_ref().and_then(|generation| {
            generation
                .prepared()
                .appearance()
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
        });
        if let Some(ThemeValue::Color(value)) = value {
            let [r, g, b] = value.rgb();
            rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        } else {
            rgb(fallback)
        }
    }

    pub(super) fn font(
        &self,
        role: &str,
        size: f32,
        weight: f32,
    ) -> (f32, gpui::FontWeight, Option<gpui::SharedString>) {
        let style = self.config.appearance.as_ref().and_then(|generation| {
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
            .and_then(|value| match value {
                ThemeValue::LogicalPixels(value) => Some(value.get()),
                _ => None,
            })
            .unwrap_or(size);
        let weight = style
            .and_then(|style| style.property(ThemePropertyId::FontWeight))
            .and_then(|value| match value {
                ThemeValue::FontWeight(value) => Some(value.get() as f32),
                _ => None,
            })
            .unwrap_or(weight);
        let family = style
            .and_then(|style| style.property(ThemePropertyId::FontFamily))
            .and_then(|value| match value {
                ThemeValue::FontFamily(value) => {
                    Some(gpui::SharedString::from(value.as_str().to_owned()))
                }
                _ => None,
            });
        (size, gpui::FontWeight(weight), family)
    }
}
