use super::*;

pub(super) fn properties(id: &str) -> Option<&'static [ThemePropertyId]> {
    Some(match id {
        "activity-panel" => SURFACE,
        "activity-panel.resize-handle.focus-visible" => COLOR,
        "activity-panel.resize-handle"
        | "activity-panel.status-marker"
        | "activity-panel.status-marker.running"
        | "activity-panel.status-marker.finished-ok"
        | "activity-panel.status-marker.finished-error" => BACKGROUND,
        "activity-panel.key" | "activity-panel.agent-value" | "activity-panel.activity-value" => {
            TEXT
        }
        "activity-panel.feedback" => &[
            ThemePropertyId::Background,
            ThemePropertyId::Foreground,
            ThemePropertyId::FontFamily,
            ThemePropertyId::FontSize,
            ThemePropertyId::FontWeight,
        ],
        _ => return None,
    })
}

pub(super) fn palette(id: &str) -> Option<Palette> {
    properties(id)?;
    let mut value = Palette {
        background: [0x0f, 0x17, 0x2a],
        border: [0x33, 0x41, 0x55],
        foreground: [0xe2, 0xe8, 0xf0],
        font_family: "Inter",
        font_size: 12.,
        font_weight: 400,
    };
    match id {
        "activity-panel.resize-handle" => value.background = [0x33, 0x41, 0x55],
        "activity-panel.resize-handle.focus-visible" => value.border = [0x38, 0xbd, 0xf8],
        "activity-panel.feedback" => {
            value.background = [0x17, 0x20, 0x33];
            value.foreground = [0xfe, 0xca, 0xca];
        }
        "activity-panel.key" => {
            value.foreground = [0x94, 0xa3, 0xb8];
            value.font_size = 11.;
            value.font_weight = 500;
        }
        "activity-panel.agent-value" => value.foreground = [0xcb, 0xd5, 0xe1],
        "activity-panel.activity-value" => value.foreground = [0xf1, 0xf5, 0xf9],
        "activity-panel.status-marker" => value.background = [0x64, 0x74, 0x8b],
        "activity-panel.status-marker.running" => value.background = [0x38, 0xbd, 0xf8],
        "activity-panel.status-marker.finished-ok" => value.background = [0x22, 0xc5, 0x5e],
        "activity-panel.status-marker.finished-error" => value.background = [0xef, 0x44, 0x44],
        _ => {}
    }
    Some(value)
}
