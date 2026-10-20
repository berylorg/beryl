use super::*;

pub(super) fn properties(id: &str) -> Option<&'static [ThemePropertyId]> {
    Some(match id {
        "segmented-status-bar" => &[
            ThemePropertyId::Background,
            ThemePropertyId::Foreground,
            ThemePropertyId::BorderColor,
            ThemePropertyId::FontSize,
        ],
        "segmented-status-bar.segment.hover"
        | "segmented-status-bar.segment.pressed"
        | "segmented-status-bar.divider" => BACKGROUND,
        "segmented-status-bar.segment.open" => BACKGROUND_FOREGROUND,
        "segmented-status-bar.segment.disabled" => FOREGROUND,
        "segmented-status-bar.key" | "segmented-status-bar.value" => {
            &[ThemePropertyId::Foreground, ThemePropertyId::FontWeight]
        }
        _ => return None,
    })
}

pub(super) fn palette(id: &str) -> Option<Palette> {
    properties(id)?;
    let mut value = Palette {
        background: [0xf8, 0xfa, 0xfc],
        foreground: [0x33, 0x41, 0x55],
        border: [0xcb, 0xd5, 0xe1],
        font_family: "Inter",
        font_size: 12.0,
        font_weight: 400,
    };
    match id {
        "segmented-status-bar.segment.hover" => value.background = [0xee, 0xf2, 0xf7],
        "segmented-status-bar.segment.pressed" => value.background = [0xe2, 0xe8, 0xf0],
        "segmented-status-bar.segment.open" => {
            value.background = [0xe2, 0xe8, 0xf0];
            value.foreground = [0x0f, 0x17, 0x2a];
        }
        "segmented-status-bar.segment.disabled" => value.foreground = [0x94, 0xa3, 0xb8],
        "segmented-status-bar.divider" => value.background = [0xcb, 0xd5, 0xe1],
        "segmented-status-bar.key" => value.foreground = [0x64, 0x74, 0x8b],
        "segmented-status-bar.value" => {
            value.foreground = [0x0f, 0x17, 0x2a];
            value.font_weight = 500;
        }
        _ => {}
    }
    Some(value)
}
