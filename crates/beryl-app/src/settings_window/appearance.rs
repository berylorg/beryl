use std::{cell::RefCell, rc::Rc, sync::Arc};

use beryl_state::{ThemePropertyId as Property, ThemeValue};
use gpui::{App, Entity, IntoElement, ParentElement, Styled, div, px, rgb};
use gpui_settings_window::{
    RgbColor, SettingsButtonStateTheme, SettingsButtonTheme, SettingsInputTheme,
    SettingsSurfaceTheme, SettingsWindowHandle, SettingsWindowOptions, SettingsWindowTheme,
};

use crate::theme_runtime::{
    AdapterFailureClass, AppearanceGeneration, AppearanceWindowAdapter, PreparedWindowAppearance,
    WindowAdapterId,
};

pub(super) struct AppearanceState {
    pub(super) generation: Arc<AppearanceGeneration>,
    pub(super) options: SettingsWindowOptions,
    pub(super) navigator: Option<Entity<super::theme_editor::ThemeRoleNavigator>>,
    pub(super) navigator_admission:
        Rc<RefCell<Option<gpui_settings_window::SettingsInteractionAdmission>>>,
    pub(super) mount_revision: u64,
    pub(super) message: Option<&'static str>,
}

pub(super) struct SettingsAppearanceAdapter {
    pub(super) id: WindowAdapterId,
    pub(super) window: SettingsWindowHandle,
    pub(super) state: Rc<RefCell<AppearanceState>>,
}

struct PreparedSettingsAppearance {
    window: SettingsWindowHandle,
    state: Rc<RefCell<AppearanceState>>,
    prior: Arc<AppearanceGeneration>,
    next: Arc<AppearanceGeneration>,
    options: SettingsWindowOptions,
    mount_revision: u64,
}

pub(super) fn options(
    options: SettingsWindowOptions,
    generation: &AppearanceGeneration,
) -> Result<SettingsWindowOptions, AdapterFailureClass> {
    let style = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == "settings.window.text")
        .map(|(_, style)| style)
        .ok_or(AdapterFailureClass::Rejected)?;
    let Some(ThemeValue::LogicalPixels(size)) = style.property(Property::FontSize) else {
        return Err(AdapterFailureClass::Rejected);
    };
    let Some(ThemeValue::FontFamily(family)) = style.property(Property::FontFamily) else {
        return Err(AdapterFailureClass::Rejected);
    };
    options
        .with_visual_theme(visual_theme(generation)?)
        .with_base_font_size(size.get())
        .and_then(|options| options.with_base_font_family(Some(family.as_str().to_owned())))
        .map_err(|_| AdapterFailureClass::Rejected)
}

pub(super) fn with_body_renderer(
    options: SettingsWindowOptions,
    state: &Rc<RefCell<AppearanceState>>,
) -> SettingsWindowOptions {
    let state = Rc::downgrade(state);
    options.with_page_body_renderer(gpui_settings_window::SettingsPageBodyRenderer::new(
        move |id, admission| {
            let state = state.upgrade()?;
            let state = state.borrow();
            if id.as_str() == "theme-role-navigator" {
                *state.navigator_admission.borrow_mut() = Some(admission);
                return state
                    .navigator
                    .clone()
                    .map(|navigator| navigator.into_any_element());
            }
            if id.as_str() == "theme-feedback" {
                let style = state
                    .generation
                    .prepared()
                    .appearance()
                    .roles()
                    .iter()
                    .find(|(id, _)| id.as_str() == "settings.window.text")
                    .map(|(_, style)| style)?;
                let Some(ThemeValue::LogicalPixels(size)) = style.property(Property::FontSize)
                else {
                    return None;
                };
                let Some(ThemeValue::FontFamily(family)) = style.property(Property::FontFamily)
                else {
                    return None;
                };
                let color = state
                    .generation
                    .prepared()
                    .appearance()
                    .roles()
                    .iter()
                    .find(|(id, _)| id.as_str() == "settings.row.label")
                    .and_then(|(_, style)| style.property(Property::Foreground));
                let Some(ThemeValue::Color(color)) = color else {
                    return None;
                };
                let [r, g, b] = color.rgb();
                return Some(
                    div()
                        .text_size(px(size.get()))
                        .font_family(family.as_str().to_owned())
                        .text_color(rgb((u32::from(r) << 16)
                            | (u32::from(g) << 8)
                            | u32::from(b)))
                        .child(state.message.unwrap_or("Loading installed themes."))
                        .into_any_element(),
                );
            }
            None
        },
    ))
}

pub(super) fn visual_theme(
    generation: &AppearanceGeneration,
) -> Result<SettingsWindowTheme, AdapterFailureClass> {
    let value = |role: &str, property| {
        generation
            .prepared()
            .appearance()
            .roles()
            .iter()
            .find(|(id, _)| id.as_str() == role)
            .and_then(|(_, style)| style.property(property))
            .ok_or_else(|| {
                tracing::warn!(
                    role,
                    ?property,
                    "Settings required appearance property is absent"
                );
                AdapterFailureClass::Rejected
            })
    };
    let color = |role, property| match value(role, property)? {
        ThemeValue::Color(color) => {
            let [r, g, b] = color.rgb();
            Ok(RgbColor::new(r, g, b))
        }
        _ => Err(AdapterFailureClass::Rejected),
    };
    let weight = |role| match value(role, Property::FontWeight)? {
        ThemeValue::FontWeight(weight) => Ok(weight.get()),
        _ => Err(AdapterFailureClass::Rejected),
    };
    let surface = |role, text, muted| -> Result<_, AdapterFailureClass> {
        Ok(SettingsSurfaceTheme {
            background: color(role, Property::Background)?,
            border: color(role, Property::Border)?,
            foreground: color(text, Property::Foreground)?,
            muted_foreground: color(muted, Property::Foreground)?,
        })
    };
    let state = |role, text| -> Result<_, AdapterFailureClass> {
        Ok(SettingsButtonStateTheme {
            background: color(role, Property::Background)?,
            border: color(role, Property::Border)?,
            foreground: color(text, Property::Foreground)?,
        })
    };
    Ok(SettingsWindowTheme {
        window_background: color("settings.window", Property::Background)?,
        panel: surface(
            "settings.page",
            "settings.group.header.text",
            "settings.row.disabled.text",
        )?,
        row: surface(
            "settings.row.normal",
            "settings.row.label",
            "settings.row.disabled.text",
        )?,
        popup: surface("settings.popup", "settings.popup", "text.muted")?,
        input: SettingsInputTheme {
            background: color("settings.input.normal", Property::Background)?,
            border: color("settings.input.normal", Property::Border)?,
            active_border: color("settings.input.focused", Property::Border)?,
            error_border: color("settings.input.error", Property::Border)?,
            foreground: color("settings.input.text", Property::Foreground)?,
            caret: color("settings.input.caret", Property::Color)?,
            selection_background: color("settings.input.selection", Property::TextBackground)?,
        },
        navigation_button: SettingsButtonTheme {
            font_weight: weight("settings.sidebar.row.text")?,
            normal: state("settings.sidebar.row.normal", "settings.sidebar.row.text")?,
            hover: SettingsButtonStateTheme {
                background: color("settings.sidebar.row.hover", Property::Background)?,
                ..state("settings.sidebar.row.normal", "settings.sidebar.row.text")?
            },
            active: state("settings.sidebar.row.selected", "settings.sidebar.row.text")?,
            disabled: state("settings.sidebar.row.normal", "settings.row.disabled.text")?,
        },
        primary_button: SettingsButtonTheme {
            font_weight: weight("settings.button.primary.label")?,
            normal: state("settings.button.primary", "settings.button.primary.label")?,
            hover: state("button.primary.hover", "settings.button.primary.label")?,
            active: state("button.primary.active", "settings.button.primary.label")?,
            disabled: state("button.primary.disabled", "settings.button.primary.label")?,
        },
        secondary_button: SettingsButtonTheme {
            font_weight: weight("settings.button.secondary.label")?,
            normal: state(
                "settings.button.secondary",
                "settings.button.secondary.label",
            )?,
            hover: state("button.secondary.hover", "settings.button.secondary.label")?,
            active: state("button.secondary.active", "settings.button.secondary.label")?,
            disabled: state(
                "button.secondary.disabled",
                "settings.button.secondary.label",
            )?,
        },
    })
}

impl AppearanceWindowAdapter for SettingsAppearanceAdapter {
    fn id(&self) -> WindowAdapterId {
        self.id
    }

    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
        cx: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, AdapterFailureClass> {
        self.window
            .entity(cx)
            .map_err(|_| AdapterFailureClass::Unavailable)?;
        let state = self.state.borrow();
        let options = options(state.options.clone(), &generation)?;
        if state.navigator.is_some() {
            super::theme_editor::ThemeRoleNavigator::validate_appearance(&generation)?;
        }
        Ok(Box::new(PreparedSettingsAppearance {
            window: self.window.clone(),
            state: self.state.clone(),
            prior: state.generation.clone(),
            next: generation,
            options,
            mount_revision: state.mount_revision,
        }))
    }
}

impl PreparedWindowAppearance for PreparedSettingsAppearance {
    fn validate(&self, cx: &mut App) -> Result<(), AdapterFailureClass> {
        self.window
            .entity(cx)
            .map_err(|_| AdapterFailureClass::Unavailable)?;
        if !Arc::ptr_eq(&self.prior, &self.state.borrow().generation)
            || self.mount_revision != self.state.borrow().mount_revision
        {
            return Err(AdapterFailureClass::Rejected);
        }
        Ok(())
    }

    fn commit(self: Box<Self>, cx: &mut App) {
        if let Some(navigator) = &self.state.borrow().navigator {
            navigator
                .update(cx, |navigator, cx| {
                    navigator.sync_appearance(self.next.clone(), cx)
                })
                .expect("prepared navigator appearance");
        }
        self.window.update_options(cx, self.options.clone()).expect(
            "validated settings appearance window remains mounted within the publication barrier",
        );
        let mut state = self.state.borrow_mut();
        state.generation = self.next;
        state.options = self.options;
    }
}
