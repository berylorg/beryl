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
    metrics: host::sizing::ShellMetrics,
}

#[cfg(any(test, feature = "test-faults"))]
impl MainWindowShellRoot {
    #[cfg(test)]
    pub(crate) fn test_appearance(&self) -> Arc<AppearanceGeneration> {
        self.controller
            .as_ref()
            .unwrap()
            .appearance
            .generation
            .clone()
    }

    pub fn test_apply_appearance(
        window: WindowHandle<Self>,
        generation: Arc<AppearanceGeneration>,
        app: &mut App,
    ) {
        let mut prepared = ShellAppearanceAdapter {
            id: WindowAdapterId::new(std::num::NonZeroU64::new(1).unwrap()),
            window,
        }
        .prepare(generation, app)
        .unwrap();
        assert!(prepared.prepare_native(app).unwrap().is_none());
        prepared.validate(app).unwrap();
        prepared.commit(app);
    }
}

impl AppearanceWindowAdapter for ShellAppearanceAdapter {
    fn id(&self) -> WindowAdapterId {
        self.id
    }
    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
        app: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        let metrics = self
            .window
            .read_with(app, |root, app| -> Result<_, AdapterFailureClass> {
                let controller = root
                    .controller
                    .as_ref()
                    .ok_or(AdapterFailureClass::Unavailable)?;
                root.candidate_metrics(
                    &generation,
                    controller.metrics.panel,
                    controller.metrics.lineage,
                    app,
                )
                .map_err(|_| AdapterFailureClass::Unavailable)
            })
            .map_err(|_| AdapterFailureClass::Unavailable)??;
        Ok(Box::new(PreparedShellAppearance {
            window: self.window,
            appearance: MainWindowShellAppearance::prepare(generation),
            metrics,
        }))
    }
}

impl PreparedWindowAppearance for PreparedShellAppearance {
    fn prepare_native(
        &mut self,
        app: &mut App,
    ) -> Result<Option<gpui::Task<Result<(), AdapterFailureClass>>>, AdapterFailureClass> {
        let compatible = self
            .window
            .read_with(app, |root, _| {
                root.controller.as_ref().is_some_and(|controller| {
                    controller.appearance.generation.prepared().home()
                        == self.appearance.generation.prepared().home()
                })
            })
            .map_err(|_| AdapterFailureClass::Unavailable)?;
        if !compatible {
            return Err(AdapterFailureClass::Rejected);
        }
        let ready = self
            .window
            .update(app, |root, window, cx| {
                root.prepare_appearance_minimum(self.metrics.clone(), window, cx)
            })
            .map_err(|_| AdapterFailureClass::Unavailable)?
            .map_err(|_| AdapterFailureClass::Unavailable)?;
        if self
            .window
            .read_with(app, |root, _| root.minimum_current(&self.metrics))
            .map_err(|_| AdapterFailureClass::Unavailable)?
        {
            return Ok(None);
        }
        Ok(Some(app.spawn(async move |_| {
            ready
                .await
                .map_err(|_| AdapterFailureClass::ApplicationFailed)
        })))
    }

    fn validate(&self, app: &mut App) -> Result<(), AdapterFailureClass> {
        self.window
            .update(app, |root, window, app| {
                let controller = root
                    .controller
                    .as_ref()
                    .ok_or(AdapterFailureClass::Unavailable)?;
                let expected = root
                    .candidate_metrics(
                        &self.appearance.generation,
                        controller.metrics.panel,
                        controller.metrics.lineage,
                        app,
                    )
                    .map_err(|_| AdapterFailureClass::Unavailable)?;
                if expected != self.metrics {
                    return Err(AdapterFailureClass::Rejected);
                }
                if !root.minimum_current(&self.metrics)
                    || window.is_minimized().is_none()
                    || !host::sizing::contains(window.viewport_size(), self.metrics.minimum())
                    || (self.metrics != controller.metrics && window.is_minimized() != Some(false))
                {
                    return Err(AdapterFailureClass::Unavailable);
                }
                if controller.appearance.generation.prepared().home()
                    != self.appearance.generation.prepared().home()
                {
                    return Err(AdapterFailureClass::Rejected);
                }
                if controller.is_threadless() {
                    return if controller.composer_mount.is_none() {
                        Ok(())
                    } else {
                        Err(AdapterFailureClass::Rejected)
                    };
                }
                let composer = controller
                    .composer_mount
                    .as_ref()
                    .and_then(|mount| mount.read(app).contribution())
                    .ok_or(AdapterFailureClass::Unavailable)?;
                if !composer.read(app).appearance_applicable(app) {
                    return Err(AdapterFailureClass::Unavailable);
                }
                Ok(())
            })
            .map_err(|_| AdapterFailureClass::Unavailable)??;
        Ok(())
    }
    fn commit(self: Box<Self>, app: &mut App) {
        let window = self.window;
        window
            .update(app, |root, window, app| {
                let controller = root
                    .controller
                    .as_mut()
                    .expect("validated shell controller");
                if let Some(composer) = controller.composer_mount.as_ref() {
                    composer
                        .update(app, |mount, cx| {
                            mount.apply_appearance(
                                self.appearance.text.clone(),
                                self.appearance.scrollbar,
                                cx,
                            )
                        })
                        .expect("validated mounted editor accepts appearance");
                }
                root.notices.widget.update(app, |widget, cx| {
                    widget.set_appearance(self.appearance.generation.clone(), window, cx);
                });
                controller.appearance = self.appearance;
                root.commit_metrics(self.metrics, window, app);
                root.appearance_registration.set(true);
                app.notify();
            })
            .expect("validated shell window");
    }
}
