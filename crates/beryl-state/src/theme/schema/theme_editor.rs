use super::*;

pub(super) fn properties(id: &str) -> Option<&'static [ThemePropertyId]> {
    Some(match id {
        "settings.window.text" => &[
            ThemePropertyId::FontFamily,
            ThemePropertyId::FontSize,
            ThemePropertyId::FontWeight,
        ],
        "theme-editor" | "theme-editor.column-header" | "theme-editor.role-row" => FOREGROUND,
        "theme-editor.navigator" | "theme-editor.column" => {
            &[ThemePropertyId::Background, ThemePropertyId::BorderColor]
        }
        "theme-editor.role-sample" => &[ThemePropertyId::BorderColor],
        "theme-editor.role-row.hover"
        | "theme-editor.role-row.pressed"
        | "theme-editor.role-row.selected" => BACKGROUND_FOREGROUND,
        "theme-editor.role-row.focused" => COLOR,
        _ => return None,
    })
}

pub(super) fn palette(id: &str) -> Option<Palette> {
    properties(id)?;
    let mut value = Palette {
        background: [0, 0, 0],
        border: [0, 0, 0],
        foreground: [0xe7, 0xe3, 0xd8],
        font_family: "Inter",
        font_size: 14.0,
        font_weight: 400,
    };
    match id {
        "theme-editor.navigator" => {
            value.background = [0x11, 0x12, 0x14];
            value.border = [0x31, 0x36, 0x3b];
        }
        "theme-editor.column" => {
            value.background = [0x1d, 0x21, 0x25];
            value.border = [0x31, 0x36, 0x3b];
        }
        "theme-editor.column-header" => value.foreground = [0x8d, 0x95, 0x9c],
        "theme-editor.role-sample" => value.border = [0x4b, 0x53, 0x5c],
        "theme-editor.role-row.focused" => value.border = [0x49, 0x96, 0x6f],
        "theme-editor.role-row.hover" => {
            value.background = [0x25, 0x2b, 0x30];
            value.foreground = [0xf3, 0xf7, 0xf4];
        }
        "theme-editor.role-row.pressed" => {
            value.background = [0x18, 0x2f, 0x3c];
            value.foreground = [0xf3, 0xf7, 0xf4];
        }
        "theme-editor.role-row.selected" => {
            value.background = [0x21, 0x48, 0x5d];
            value.foreground = [0xf3, 0xf7, 0xf4];
        }
        _ => {}
    }
    Some(value)
}
