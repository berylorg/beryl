use beryl_state::{ResolvedAppearance, ThemePropertyId as Property, ThemeValue};
use gpui::{Font, FontWeight, Rgba, SharedString, font, rgb};

#[derive(Clone)]
pub(super) struct ContextAppearance {
    pub(super) background: Rgba,
    pub(super) border: Rgba,
    pub(super) foreground: Rgba,
    pub(super) heading: ContextTextAppearance,
    pub(super) body: ContextTextAppearance,
    pub(super) provenance: ContextTextAppearance,
}

#[derive(Clone)]
pub(super) struct ContextTextAppearance {
    pub(super) font: Font,
    pub(super) size: f32,
    pub(super) color: Rgba,
}

impl Default for ContextAppearance {
    fn default() -> Self {
        let text = |size, color, weight| {
            let mut font = font("Inter");
            font.weight = FontWeight(weight);
            ContextTextAppearance {
                font,
                size,
                color: rgb(color),
            }
        };
        Self {
            background: rgb(0x111827),
            border: rgb(0x475569),
            foreground: rgb(0xe2e8f0),
            heading: text(10.0, 0x7dd3fc, 700.0),
            body: text(14.0, 0xe2e8f0, 400.0),
            provenance: text(11.0, 0x94a3b8, 400.0),
        }
    }
}

impl ContextAppearance {
    pub(super) fn prepare(appearance: &ResolvedAppearance) -> Self {
        let property = |role, property| {
            appearance
                .roles()
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .expect("canonical context appearance property")
        };
        let color = |role, kind| {
            let ThemeValue::Color(value) = property(role, kind) else {
                unreachable!("canonical context color")
            };
            let [r, g, b] = value.rgb();
            rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        };
        let text = |role| {
            let ThemeValue::FontFamily(family) = property(role, Property::FontFamily) else {
                unreachable!("canonical context font")
            };
            let ThemeValue::LogicalPixels(size) = property(role, Property::FontSize) else {
                unreachable!("canonical context size")
            };
            let ThemeValue::FontWeight(weight) = property(role, Property::FontWeight) else {
                unreachable!("canonical context weight")
            };
            let mut font = font(SharedString::from(family.as_str().to_owned()));
            font.weight = FontWeight(f32::from(weight.get()));
            ContextTextAppearance {
                font,
                size: size.get(),
                color: color(role, Property::Foreground),
            }
        };
        Self {
            background: color("transcript-view.context-group", Property::Background),
            border: color("transcript-view.context-group", Property::Border),
            foreground: color("transcript-view.context-group", Property::Foreground),
            heading: text("transcript-view.context-heading"),
            body: text("transcript-view.context-body"),
            provenance: text("transcript-view.context-provenance"),
        }
    }

    pub(super) fn scale(&self) -> f32 {
        self.body.size / 14.0
    }
}
