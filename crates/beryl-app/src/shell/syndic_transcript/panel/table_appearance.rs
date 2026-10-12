use beryl_state::{ResolvedAppearance, ThemePropertyId as Property, ThemeValue};
use gpui::{Font, FontWeight, Rgba, SharedString, font, rgb};
#[derive(Clone)]
pub(super) struct TableAppearance {
    pub background: Rgba,
    pub foreground: Rgba,
    pub border: Rgba,
    pub header: Rgba,
    pub header_foreground: Rgba,
    pub header_border: Rgba,
    pub cell_background: Rgba,
    pub cell_border: Rgba,
    pub cell_foreground: Rgba,
    pub header_cell_background: Rgba,
    pub header_cell_foreground: Rgba,
    pub header_cell_border: Rgba,
    pub selected: Rgba,
    pub selected_foreground: Rgba,
    pub focused: Rgba,
    pub unavailable: Rgba,
    pub pending: Rgba,
    pub font: Font,
    pub font_size: f32,
    pub header_font: Font,
    pub header_font_size: f32,
}
impl Default for TableAppearance {
    fn default() -> Self {
        Self {
            background: rgb(0x0f172a),
            foreground: rgb(0xe5e7eb),
            border: rgb(0x475569),
            header: rgb(0x0f172a),
            header_foreground: rgb(0xe5e7eb),
            header_border: rgb(0x334155),
            cell_background: rgb(0x0f172a),
            cell_border: rgb(0x334155),
            cell_foreground: rgb(0xdbe4ef),
            header_cell_background: rgb(0x111827),
            header_cell_foreground: rgb(0xf1f5f9),
            header_cell_border: rgb(0x334155),
            selected: rgb(0x1e3a5f),
            selected_foreground: rgb(0xf8fafc),
            focused: rgb(0x38bdf8),
            unavailable: rgb(0x94a3b8),
            pending: rgb(0x94a3b8),
            font: font("Segoe UI"),
            font_size: 13.0,
            header_font: font("Segoe UI"),
            header_font_size: 13.0,
        }
    }
}
impl TableAppearance {
    pub(super) fn prepare(appearance: &ResolvedAppearance) -> Self {
        let property = |role, kind| {
            appearance
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(kind))
                .expect("canonical Table appearance")
        };
        let color = |role, kind| {
            let ThemeValue::Color(value) = property(role, kind) else {
                unreachable!()
            };
            let [r, g, b] = value.rgb();
            rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        };
        let typography = |role| {
            let ThemeValue::FontFamily(family) = property(role, Property::FontFamily) else {
                unreachable!()
            };
            let ThemeValue::LogicalPixels(size) = property(role, Property::FontSize) else {
                unreachable!()
            };
            let ThemeValue::FontWeight(weight) = property(role, Property::FontWeight) else {
                unreachable!()
            };
            let mut font = font(SharedString::from(family.as_str().to_owned()));
            font.weight = FontWeight(f32::from(weight.get()));
            (font, size.get())
        };
        let (font, font_size) = typography("table-panel.cell");
        let (header_font, header_font_size) = typography("table-panel.header-cell");
        Self {
            background: color("table-panel", Property::Background),
            foreground: color("table-panel", Property::Foreground),
            border: color("table-panel", Property::Border),
            header: color("table-panel.header", Property::Background),
            header_foreground: color("table-panel.header", Property::Foreground),
            header_border: color("table-panel.header", Property::Border),
            cell_background: color("table-panel.cell", Property::Background),
            cell_border: color("table-panel.cell", Property::Border),
            cell_foreground: color("table-panel.cell", Property::Foreground),
            header_cell_background: color("table-panel.header-cell", Property::Background),
            header_cell_foreground: color("table-panel.header-cell", Property::Foreground),
            header_cell_border: color("table-panel.header-cell", Property::Border),
            selected: color("table-panel.cell.selected", Property::Background),
            selected_foreground: color("table-panel.cell.selected", Property::Foreground),
            focused: color("table-panel.cell.focused", Property::Color),
            unavailable: color("table-panel.cell.unavailable", Property::Foreground),
            pending: color("table-panel.cell.pending", Property::Foreground),
            font,
            font_size,
            header_font,
            header_font_size,
        }
    }
    pub(super) fn row_height(&self, cx: &gpui::App) -> Option<f32> {
        let system = cx.text_system();
        let key = system.font_environment_key()?;
        let mut height = 28.0_f32;
        for (font, font_size) in [
            (&self.font, self.font_size),
            (&self.header_font, self.header_font_size),
        ] {
            let id = system.try_resolve_font(font).ok()?;
            let size = gpui::px(font_size);
            height = height.max(
                (f32::from(system.ascent(id, size)).abs()
                    + f32::from(system.descent(id, size)).abs())
                .ceil()
                    + 11.0,
            );
        }
        (height.is_finite() && system.font_environment_key() == Some(key))
            .then_some(height.max(28.0))
    }
    pub(super) fn column_width(&self, available: f32) -> f32 {
        (12.0 * self.font_size + 17.0).max((available / 7.0).ceil())
    }
    pub(super) fn viewport_height(&self, row_height: f32) -> f32 {
        (10.0 * row_height).min(360.0)
    }
    pub(super) fn header_height(&self) -> f32 {
        32.0_f32.max(self.header_font_size * 1.5 + 10.0)
    }
}
