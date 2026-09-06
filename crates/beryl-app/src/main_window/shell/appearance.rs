use super::*;
use crate::theme_runtime::{
    AdapterFailureClass, AppearanceWindowAdapter, PreparedWindowAppearance, WindowAdapterId,
};
use beryl_state::{ThemePropertyId as Property, ThemeValue};

#[derive(Clone)]
pub struct MainWindowShellAppearance {
    pub generation: Arc<AppearanceGeneration>,
    pub background: gpui::Rgba,
    pub toolbar: gpui::Rgba,
    pub status: gpui::Rgba,
    pub input_panel: gpui::Rgba,
    pub separator: gpui::Rgba,
    pub text: gpui_text_input::TextInputTheme,
    pub scrollbar: gpui_scrollbar::ScrollbarStyle,
}

impl MainWindowShellAppearance {
    pub fn prepare(generation: Arc<AppearanceGeneration>) -> Self {
        let color = |role, property| {
            let roles = generation.prepared().appearance().roles();
            let value = roles
                .iter()
                .find(|(id, _)| id.as_str() == role)
                .and_then(|(_, style)| style.property(property))
                .expect("canonical shell theme property");
            let ThemeValue::Color(value) = value else {
                unreachable!("canonical color property")
            };
            let [r, g, b] = value.rgb();
            gpui::rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        };
        let text = gpui_text_input::TextInputTheme {
            text: Some(color("input.field.text", Property::Foreground).into()),
            placeholder: color("input.field.text", Property::Foreground).into(),
            selection: color("input.selection", Property::TextBackground).into(),
            caret: color("input.caret", Property::Color).into(),
            marked_underline: color("input.field.text", Property::Foreground).into(),
            atom_text: color("input.field.text", Property::Foreground).into(),
            atom_background: Some(color("input.field", Property::Background).into()),
        };
        let thumb = color("scrollbar.thumb.normal", Property::Color);
        Self {
            background: color("app.window", Property::Background),
            toolbar: color("main.toolbar", Property::Background),
            status: color("status.line", Property::Background),
            input_panel: color("input.panel", Property::Background),
            separator: color("main.separator", Property::Color),
            text,
            scrollbar: gpui_scrollbar::ScrollbarStyle {
                thumb_color: ((thumb.r * 255.).round() as u32) << 16
                    | ((thumb.g * 255.).round() as u32) << 8
                    | (thumb.b * 255.).round() as u32,
                ..Default::default()
            },
            generation,
        }
    }
}

pub(super) struct ShellAppearanceAdapter {
    pub id: WindowAdapterId,
    pub window: WindowHandle<MainWindowShellRoot>,
}

struct PreparedShellAppearance {
    window: WindowHandle<MainWindowShellRoot>,
    appearance: MainWindowShellAppearance,
}

impl AppearanceWindowAdapter for ShellAppearanceAdapter {
    fn id(&self) -> WindowAdapterId {
        self.id
    }
    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
        _: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        Ok(Box::new(PreparedShellAppearance {
            window: self.window,
            appearance: MainWindowShellAppearance::prepare(generation),
        }))
    }
}

impl PreparedWindowAppearance for PreparedShellAppearance {
    fn validate(&self, app: &App) -> Result<(), AdapterFailureClass> {
        self.window
            .read_with(app, |root, app| {
                let controller = root
                    .controller
                    .as_ref()
                    .ok_or(AdapterFailureClass::Unavailable)?;
                if controller.appearance.generation.prepared().home()
                    != self.appearance.generation.prepared().home()
                {
                    return Err(AdapterFailureClass::Rejected);
                }
                let composer = controller
                    .composer_mount
                    .as_ref()
                    .and_then(|mount| mount.read(app).contribution())
                    .ok_or(AdapterFailureClass::Unavailable)?;
                if !composer.read(app).appearance_applicable() {
                    return Err(AdapterFailureClass::Unavailable);
                }
                Ok(())
            })
            .map_err(|_| AdapterFailureClass::Unavailable)?
    }
    fn commit(self: Box<Self>, app: &mut App) {
        let window = self.window;
        window
            .update(app, |root, window, app| {
                let controller = root
                    .controller
                    .as_mut()
                    .expect("validated shell controller");
                controller
                    .composer_mount
                    .as_ref()
                    .expect("validated shell composer")
                    .update(app, |mount, cx| {
                        mount.apply_appearance(
                            self.appearance.text.clone(),
                            self.appearance.scrollbar,
                            cx,
                        )
                    })
                    .expect("validated mounted editor accepts appearance");
                root.notices.widget.update(app, |widget, cx| {
                    widget.set_appearance(self.appearance.generation.clone(), window, cx);
                });
                controller.appearance = self.appearance;
                app.notify();
            })
            .expect("validated shell window");
    }
}
