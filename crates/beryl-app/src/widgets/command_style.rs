use gpui::{FontWeight, Rgba, rgb};

pub(crate) const ROUNDED_WIDGET_CORNER_RADIUS: f32 = 6.0;
pub(crate) const BUTTON_OUTER_HEIGHT: f32 = 32.0;
pub(crate) const BUTTON_LABEL_FONT_SIZE: f32 = 14.0;
pub(crate) const BUTTON_LABEL_LINE_HEIGHT: f32 = 18.0;
pub(crate) const BUTTON_BORDER_WIDTH: f32 = 1.0;
pub(crate) const BUTTON_VERTICAL_PADDING: f32 = 6.0;
pub(crate) const BUTTON_HORIZONTAL_PADDING: f32 = 10.0;

pub(crate) const fn button_required_outer_height() -> f32 {
    BUTTON_BORDER_WIDTH * 2.0 + BUTTON_VERTICAL_PADDING * 2.0 + BUTTON_LABEL_LINE_HEIGHT
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeButtonTheme {
    pub(crate) font_weight: FontWeight,
    pub(crate) normal: ChromeButtonStateTheme,
    pub(crate) hover: ChromeButtonStateTheme,
    pub(crate) active: ChromeButtonStateTheme,
    pub(crate) disabled: ChromeButtonStateTheme,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeButtonStateTheme {
    pub(crate) background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) foreground: Rgba,
}

impl ChromeButtonTheme {
    pub(crate) fn secondary() -> Self {
        Self {
            font_weight: FontWeight(500.0),
            normal: ChromeButtonStateTheme::new(rgb(0x1e293b), rgb(0x475569), rgb(0xe2e8f0)),
            hover: ChromeButtonStateTheme::new(rgb(0x334155), rgb(0x64748b), rgb(0xf8fafc)),
            active: ChromeButtonStateTheme::new(rgb(0x0f172a), rgb(0x475569), rgb(0xf8fafc)),
            disabled: ChromeButtonStateTheme::new(rgb(0x111827), rgb(0x334155), rgb(0x64748b)),
        }
    }
}

impl ChromeButtonStateTheme {
    pub(crate) fn new(background: Rgba, border: Rgba, foreground: Rgba) -> Self {
        Self {
            background,
            border,
            foreground,
        }
    }
}
