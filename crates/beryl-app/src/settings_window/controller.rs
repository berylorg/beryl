use std::{cell::RefCell, num::NonZeroU64, rc::Rc, sync::Arc};

use beryl_state::SettingKey;
use gpui::{App, AppContext as _, Context, Entity, Subscription, Task, WeakEntity};
use gpui_settings_window::{
    SettingsFieldInputRefusal, SettingsWindowEvent, SettingsWindowHandle, SettingsWindowModel,
    SettingsWindowOpenDisposition, SettingsWindowOptions, open_settings_window,
};

use crate::{
    settings_service::{SettingsDraft, SettingsDraftValue, SettingsIssue, SettingsService},
    theme_runtime::{
        AppearanceGeneration, GpuiAppearanceWindowSet, SettingsThemeReader, WindowAdapterId,
    },
};

use super::{
    appearance::{self, AppearanceState, SettingsAppearanceAdapter},
    fields::{self, Route},
    presentation,
};

mod commands;
mod qualification;

#[derive(Debug, thiserror::Error)]
pub enum SettingsWindowMountError {
    #[error("The native Settings window is unavailable")]
    Window,
    #[error("The Settings appearance could not join the current window set")]
    Appearance,
    #[error("Settings is waiting for its original operation to finish")]
    Busy,
    #[error("The Settings draft identity is exhausted")]
    Identity,
    #[error("The Settings service belongs to another home")]
    ForeignHome,
}

#[derive(Clone)]
pub struct SettingsWindowCommand {
    owner: WeakEntity<SettingsWindowController>,
}

impl SettingsWindowCommand {
    pub fn is_alive(&self) -> bool {
        self.owner.upgrade().is_some()
    }

    pub fn disabled_reason(&self, app: &App) -> Option<&'static str> {
        let Some(owner) = self.owner.upgrade() else {
            return Some("Settings is unavailable.");
        };
        owner
            .read(app)
            .retired
            .then_some("Settings is unavailable while the application is closing.")
    }

    pub fn open(&self, app: &mut App) -> Result<(), SettingsWindowMountError> {
        self.owner
            .upgrade()
            .ok_or(SettingsWindowMountError::Window)?
            .update(app, |owner, cx| owner.open(cx))
    }
}

pub struct SettingsWindowController {
    pub(super) service: Arc<SettingsService>,
    pub(super) themes: SettingsThemeReader,
    pub(super) window: Option<SettingsWindowHandle>,
    appearance_owner: Entity<GpuiAppearanceWindowSet>,
    adapter_id: WindowAdapterId,
    pub(super) appearance: Rc<RefCell<AppearanceState>>,
    pub(super) draft: Option<SettingsDraft>,
    pub(super) window_instance: u64,
    pub(super) theme_state: super::themes::ThemesState,
    pub(super) theme_pulse: Option<Task<()>>,
    pub(super) route: Route,
    pub(super) source_current: bool,
    pub(super) home_available: bool,
    pub(super) feedback: [Option<String>; 5],
    pub(super) pending: Option<commands::PendingApply>,
    pub(super) worker: Option<Task<()>>,
    pub(super) confirmed_theme: Option<crate::theme_runtime::ConfirmedSettingsTheme>,
    pub(super) appearance_busy: bool,
    pub(super) appearance_failed: bool,
    pub(super) appearance_retry: bool,
    pub(super) close_after_appearance: bool,
    pub(super) repository_appearance_pending: bool,
    pub(super) repository_appearance_ticket: Option<crate::theme_runtime::ThemeSettingsTicket>,
    pub(super) theme_qualification: Option<(
        crate::settings_service::SettingsDraftBinding,
        crate::theme_runtime::ConfirmedSettingsTheme,
    )>,
    pub(super) theme_qualification_error: Option<crate::settings_service::SettingsDraftBinding>,
    pub(super) theme_qualification_worker: Option<Task<()>>,
    pub(super) editor_revision: u64,
    pub(super) editor_worker: Option<Task<()>>,
    pub(super) navigator_subscription: Option<Subscription>,
    subscription: Option<Subscription>,
    subscription_binding: Option<crate::settings_service::SettingsDraftBinding>,
    pub(super) retired: bool,
}

impl SettingsWindowController {
    pub fn preheat(
        service: Arc<SettingsService>,
        themes: SettingsThemeReader,
        appearance_owner: Entity<GpuiAppearanceWindowSet>,
        initial: Arc<AppearanceGeneration>,
        app: &mut App,
    ) -> Result<Entity<Self>, SettingsWindowMountError> {
        let options = appearance::options(SettingsWindowOptions::new("Settings"), &initial)
            .map_err(|_| SettingsWindowMountError::Appearance)?;
        let baseline = service.reader().snapshot().ok();
        if themes.home() != initial.home()
            || baseline.as_ref().is_some_and(|baseline| {
                baseline.identity().home_id != initial.home().home_id()
                    || baseline.identity().home_generation != initial.home().home_generation()
            })
        {
            return Err(SettingsWindowMountError::ForeignHome);
        }
        let owner = app.new(|cx| {
            let instance = cx.entity_id().as_u64();
            cx.on_release(|owner: &mut Self, app| owner.release_window(app))
                .detach();
            Self {
                service,
                themes,
                window: None,
                appearance_owner,
                adapter_id: WindowAdapterId::new(
                    NonZeroU64::new(instance).expect("GPUI entity identity"),
                ),
                appearance: Rc::new(RefCell::new(AppearanceState {
                    generation: initial,
                    options: options.clone(),
                    navigator: None,
                    navigator_admission: Rc::new(RefCell::new(None)),
                    mount_revision: 0,
                    message: None,
                })),
                draft: baseline.map(|baseline| {
                    SettingsDraft::new(baseline, instance, Route::Themes.binding())
                }),
                window_instance: instance,
                theme_state: Default::default(),
                theme_pulse: None,
                route: Route::Themes,
                source_current: true,
                home_available: true,
                feedback: std::array::from_fn(|_| None),
                pending: None,
                worker: None,
                confirmed_theme: None,
                appearance_busy: false,
                appearance_failed: false,
                appearance_retry: false,
                close_after_appearance: false,
                repository_appearance_pending: false,
                repository_appearance_ticket: None,
                theme_qualification: None,
                theme_qualification_error: None,
                theme_qualification_worker: None,
                editor_revision: 0,
                editor_worker: None,
                navigator_subscription: None,
                subscription: None,
                subscription_binding: None,
                retired: false,
            }
        });
        let options = owner.update(app, |owner, _| {
            let mut state = owner.appearance.borrow_mut();
            state.options =
                appearance::with_body_renderer(state.options.clone(), &owner.appearance);
            state.options.clone()
        });
        let model = owner.read(app).model();
        let handle =
            open_settings_window(app, model, options, SettingsWindowOpenDisposition::Hidden)
                .map_err(|_| SettingsWindowMountError::Window)?;
        owner.update(app, |owner, cx| {
            owner.window = Some(handle.clone());
            owner.bind_window_events(cx);
        });
        let (adapter_id, state, appearance_owner) = owner.read_with(app, |owner, _| {
            (
                owner.adapter_id,
                owner.appearance.clone(),
                owner.appearance_owner.clone(),
            )
        });
        appearance_owner
            .update(app, |set, cx| {
                set.register(
                    Box::new(SettingsAppearanceAdapter {
                        id: adapter_id,
                        window: handle,
                        state,
                    }),
                    cx,
                )
            })
            .map_err(|_| SettingsWindowMountError::Appearance)?;
        Ok(owner)
    }

    pub fn command(owner: &Entity<Self>) -> SettingsWindowCommand {
        SettingsWindowCommand {
            owner: owner.downgrade(),
        }
    }

    pub fn native_window(&self) -> Option<&SettingsWindowHandle> {
        self.window.as_ref()
    }

    pub fn exit_is_gated(&self) -> bool {
        self.pending.is_some()
            || self.service.is_reconciling()
            || self.theme_state.is_pending()
            || self.appearance_busy
    }

    pub fn take_confirmed_theme(&mut self) -> Option<crate::theme_runtime::ConfirmedSettingsTheme> {
        self.confirmed_theme.take()
    }

    pub fn theme_role_navigator(&self) -> Option<Entity<super::theme_editor::ThemeRoleNavigator>> {
        self.appearance.borrow().navigator.clone()
    }

    pub fn set_appearance_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.appearance_busy = busy;
        if !busy {
            self.repository_appearance_pending = false;
            self.repository_appearance_ticket = None;
        }
        self.publish(cx);
        if !busy && std::mem::take(&mut self.close_after_appearance) {
            self.deactivate_theme_editor(cx);
            self.theme_state.editor = None;
            self.editor_worker = None;
            self.retire_theme_reads();
            if let Some(window) = &self.window {
                let _ = window.hide(cx);
            }
        }
    }

    pub fn report_appearance_failure(&mut self, cx: &mut Context<Self>) {
        self.appearance_failed = true;
        self.publish(cx);
    }

    pub fn clear_appearance_failure(&mut self, cx: &mut Context<Self>) {
        self.appearance_failed = false;
        self.publish(cx);
    }

    pub fn take_appearance_retry(&mut self) -> bool {
        std::mem::take(&mut self.appearance_retry)
    }

    pub fn set_home_available(&mut self, available: bool, cx: &mut Context<Self>) {
        let fresh = self.service.reader().snapshot().ok();
        let source_changed = fresh
            .as_ref()
            .zip(self.draft.as_ref())
            .is_some_and(|(baseline, draft)| baseline.identity() != draft.binding().source);
        if self.home_available == available && !source_changed {
            return;
        }
        self.home_available = available;
        if source_changed && !self.exit_is_gated() {
            self.source_current = false;
            if self
                .draft
                .as_ref()
                .is_some_and(|draft| draft.modified().is_empty())
            {
                if let Some(fresh) = fresh {
                    self.source_current = self.discard_to(fresh).is_ok();
                }
            }
        }
        self.publish(cx);
    }

    pub fn replace_service(
        &mut self,
        service: Arc<SettingsService>,
        themes: SettingsThemeReader,
        cx: &mut Context<Self>,
    ) -> Result<(), SettingsWindowMountError> {
        if let Some(pending) = &self.pending {
            if let Ok(Some(observation)) = service.observation(pending.operation) {
                if !matches!(
                    observation,
                    crate::settings_service::SettingsApplyObservation::Reconciling { .. }
                ) {
                    self.consume_observation(observation, cx);
                }
            }
            if self.pending.is_some() {
                return Err(SettingsWindowMountError::Busy);
            }
            self.worker = None;
        }
        let fresh = service.reader().snapshot().ok();
        if fresh.as_ref().is_some_and(|baseline| {
            baseline.identity().home_id != themes.home().home_id()
                || baseline.identity().home_generation != themes.home().home_generation()
        }) {
            return Err(SettingsWindowMountError::ForeignHome);
        }
        if self
            .draft
            .as_ref()
            .zip(fresh.as_ref())
            .is_some_and(|(draft, fresh)| {
                draft.binding().source.home_id != fresh.identity().home_id
            })
        {
            return Err(SettingsWindowMountError::ForeignHome);
        }
        self.deactivate_theme_editor(cx);
        self.retire_theme_reads();
        self.theme_qualification_worker = None;
        self.theme_qualification = None;
        self.theme_qualification_error = None;
        let editor = self.theme_state.editor.take();
        let epoch = self
            .theme_state
            .epoch
            .checked_add(1)
            .ok_or(SettingsWindowMountError::Identity)?;
        self.theme_state = Default::default();
        self.theme_state.epoch = epoch;
        self.theme_state.repository_unavailable = editor.is_some();
        self.theme_state.editor = editor;
        self.service = service;
        self.themes = themes;
        self.source_current = false;
        if self
            .draft
            .as_ref()
            .is_none_or(|draft| draft.modified().is_empty())
        {
            if let Some(fresh) = fresh {
                self.discard_to(fresh)?;
                self.source_current = true;
            }
        }
        self.publish(cx);
        Ok(())
    }

    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> Result<bool, SettingsWindowMountError> {
        if self.exit_is_gated() {
            return Ok(false);
        }
        self.retired = true;
        self.release_window(cx);
        Ok(true)
    }

    pub fn drain(&mut self, cx: &mut Context<Self>) -> Result<bool, SettingsWindowMountError> {
        self.publish(cx);
        Ok(!self.exit_is_gated())
    }

    pub(super) fn model(&self) -> SettingsWindowModel {
        presentation::model(self.draft.as_ref(), self.route, self.source_current && self.home_available,
            self.exit_is_gated().then_some("The original Settings operation is reconciling. Its outcome must be proven before another action."), &self.feedback, &self.theme_state, self.window_instance, self.appearance_failed, self.theme_is_qualified(), self.theme_qualification_feedback())
    }

    fn bind_window_events(&mut self, cx: &mut Context<Self>) {
        let binding = self.draft.as_ref().map(|draft| draft.binding());
        if self.subscription.is_some() && self.subscription_binding == binding {
            return;
        }
        self.subscription = None;
        let Some(window) = &self.window else { return };
        let Ok(view) = window.entity(cx) else { return };
        self.subscription_binding = binding;
        let instance = self.window_instance;
        self.subscription = Some(cx.subscribe(
            &view,
            move |owner, view, event: &SettingsWindowEvent, cx| {
                if owner.window_instance != instance
                    || owner.draft.as_ref().map(|draft| draft.binding()) != binding
                    || owner.window.as_ref().is_none_or(|window| {
                        !window
                            .entity(cx)
                            .is_ok_and(|current| current.entity_id() == view.entity_id())
                            || (!window.is_visible(cx).unwrap_or(false)
                                && !matches!(event, SettingsWindowEvent::CloseRequested))
                    })
                {
                    return;
                }
                owner.handle_event(event.clone(), cx);
            },
        ));
    }

    pub(super) fn publish(&mut self, cx: &mut Context<Self>) {
        self.drive_theme_qualification(cx);
        self.appearance.borrow_mut().message = self.theme_state.message;
        if let Some(window) = &self.window {
            if window.update_model(cx, self.model()).is_err() {
                tracing::warn!("Settings presentation observer is unavailable");
            }
        }
        self.bind_window_events(cx);
        cx.notify();
    }

    fn discard_to(
        &mut self,
        baseline: Arc<crate::settings_service::SettingsBaseline>,
    ) -> Result<(), SettingsWindowMountError> {
        if let Some(draft) = &mut self.draft {
            draft
                .discard_to_baseline(baseline)
                .map_err(|_| SettingsWindowMountError::Identity)?;
        } else {
            self.draft = Some(SettingsDraft::new(
                baseline,
                self.window_instance,
                self.route.binding(),
            ));
        }
        self.feedback = std::array::from_fn(|_| None);
        Ok(())
    }

    fn open(&mut self, cx: &mut Context<Self>) -> Result<(), SettingsWindowMountError> {
        if self.retired {
            return Err(SettingsWindowMountError::Window);
        }
        let window = self
            .window
            .clone()
            .ok_or(SettingsWindowMountError::Window)?;
        if !window
            .is_visible(cx)
            .map_err(|_| SettingsWindowMountError::Window)?
            && !self.exit_is_gated()
        {
            self.retire_theme_reads();
            self.theme_state.catalog = None;
            self.theme_state.read_failed = false;
            if let Ok(baseline) = self.service.reader().snapshot() {
                self.discard_to(baseline)?;
                self.source_current = true;
            } else {
                self.source_current = false;
            }
        }
        window
            .show(cx, self.model(), true)
            .map_err(|_| SettingsWindowMountError::Window)?;
        self.bind_window_events(cx);
        self.drive_theme_reads(cx);
        Ok(())
    }

    pub(super) fn set_feedback(&mut self, key: SettingKey, text: String) {
        let index = fields::KEYS
            .iter()
            .position(|candidate| *candidate == key)
            .unwrap();
        self.feedback[index] = Some(text);
    }

    fn handle_event(&mut self, event: SettingsWindowEvent, cx: &mut Context<Self>) {
        if self.retired || self.exit_is_gated() {
            return;
        }
        let model = self.model();
        match &event {
            SettingsWindowEvent::FieldChanged { field_id, .. }
            | SettingsWindowEvent::FieldInputRefused { field_id, .. } => {
                if !model.selected_rows().iter().any(|row| {
                    row.field_id() == field_id
                        || row
                            .detail_field()
                            .is_some_and(|detail| detail.field_id() == field_id)
                }) {
                    return;
                }
            }
            SettingsWindowEvent::PageActionRequested { page_id, .. }
                if page_id.as_str() != self.route.id() =>
            {
                return;
            }
            SettingsWindowEvent::RowActionRequested {
                field_id,
                action_id,
            } => {
                if !model.selected_rows().iter().any(|row| {
                    row.field_id() == field_id
                        && row
                            .actions()
                            .iter()
                            .any(|action| action.action_id() == action_id && action.is_enabled())
                }) {
                    return;
                }
            }
            _ => {}
        }
        match event {
            SettingsWindowEvent::FieldChanged { field_id, value } => {
                let Some(key) = fields::key(field_id.as_str()) else {
                    self.handle_theme_field(field_id.as_str(), value, cx);
                    return;
                };
                if !self.source_current || !self.home_available || key == SettingKey::ActiveThemeId
                {
                    return;
                }
                let Some(draft) = &mut self.draft else {
                    return;
                };
                let value = match key {
                    SettingKey::ActiveThemeId => return,
                    SettingKey::ContextCompactionTimeout => {
                        SettingsDraftValue::ContextCompactionTimeout(value)
                    }
                    SettingKey::DraftAutosaveInterval => {
                        SettingsDraftValue::DraftAutosaveInterval(value)
                    }
                    SettingKey::DeveloperInstructions => {
                        SettingsDraftValue::DeveloperInstructions(value)
                    }
                    SettingKey::EndTurnSound => SettingsDraftValue::EndTurnSound(value),
                };
                if draft.set(value).is_ok() {
                    let index = fields::KEYS
                        .iter()
                        .position(|candidate| *candidate == key)
                        .unwrap();
                    self.feedback[index] = None;
                }
            }
            SettingsWindowEvent::FieldInputRefused {
                field_id,
                reason:
                    SettingsFieldInputRefusal::ValueByteLimitExceeded {
                        limit,
                        attempted_bytes,
                    },
            } => {
                if let Some(key) = fields::key(field_id.as_str()) {
                    self.set_feedback(key, format!("Input was refused before changing the draft: {attempted_bytes} UTF-8 bytes would exceed the {limit}-byte editor limit."));
                } else if let Some(editor) = &mut self.theme_state.editor {
                    editor.feedback = Some((
                        field_id.as_str().to_owned(),
                        "Input was refused before changing the theme draft because its field limit would be exceeded.",
                    ));
                } else {
                    self.theme_state.message = Some(
                        "Input was refused before changing the theme draft because its field limit would be exceeded.",
                    );
                }
            }
            SettingsWindowEvent::RowActionRequested {
                field_id,
                action_id,
            } if action_id.as_str() == "reset" => {
                if !self.source_current || !self.home_available {
                    return;
                }
                let Some(key) = fields::key(field_id.as_str()) else {
                    return;
                };
                if self
                    .draft
                    .as_mut()
                    .is_some_and(|draft| draft.reset(key).is_ok())
                {
                    let index = fields::KEYS
                        .iter()
                        .position(|candidate| *candidate == key)
                        .unwrap();
                    self.feedback[index] = None;
                }
            }
            SettingsWindowEvent::SectionSelected { section_id } => {
                if self.route == Route::ThemeEditor {
                    self.deactivate_theme_editor(cx);
                }
                self.change_route(section_id.as_str());
            }
            SettingsWindowEvent::PageNavigationRequested { page_id } => {
                if page_id.as_str() != "theme-editor" {
                    self.deactivate_theme_editor(cx);
                }
                self.change_route(page_id.as_str());
            }
            SettingsWindowEvent::PageSplitItemSelected { page_id, item_id }
                if page_id.as_str() == "themes" =>
            {
                self.select_installed_theme(item_id.as_str(), cx);
                return;
            }
            SettingsWindowEvent::PageActionRequested { action_id, .. }
                if action_id.as_str() == "appearance.retry" =>
            {
                self.appearance_retry = true;
                self.appearance_busy = true;
            }
            SettingsWindowEvent::PageActionRequested { action_id, .. } => {
                self.theme_action(action_id.as_str(), cx);
                return;
            }
            SettingsWindowEvent::RowActionRequested { action_id, .. } => {
                if action_id.as_str() == "appearance.retry" {
                    self.appearance_retry = true;
                    self.appearance_busy = true;
                    self.publish(cx);
                    return;
                }
                self.theme_action(action_id.as_str(), cx);
                return;
            }
            SettingsWindowEvent::ApplyRequested => {
                self.apply(false, cx);
                return;
            }
            SettingsWindowEvent::AcceptRequested => {
                self.apply(true, cx);
                return;
            }
            SettingsWindowEvent::CancelRequested | SettingsWindowEvent::CloseRequested => {
                self.cancel(cx);
                return;
            }
            _ => return,
        }
        self.publish(cx);
    }

    pub(super) fn change_route(&mut self, id: &str) {
        if let Some(route) = Route::from_id(id) {
            if let Some(draft) = &mut self.draft {
                if draft.set_route(route.binding()).is_err() {
                    return;
                }
            }
            self.route = route;
            if route != Route::ThemeEditor {
                *self.appearance.borrow().navigator_admission.borrow_mut() = None;
            }
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.exit_is_gated() {
            return;
        }
        self.deactivate_theme_editor(cx);
        self.theme_state.editor = None;
        self.editor_worker = None;
        self.theme_qualification_worker = None;
        self.theme_qualification = None;
        self.theme_qualification_error = None;
        self.retire_theme_reads();
        let fresh = self.service.reader().snapshot().ok();
        let baseline = fresh
            .clone()
            .or_else(|| self.draft.as_ref().map(|draft| draft.baseline().clone()));
        if let Some(baseline) = baseline {
            if self.discard_to(baseline).is_err() {
                return;
            }
            self.source_current = fresh.is_some();
        }
        if let Some(window) = &self.window {
            let _ = window.hide(cx);
        }
    }

    fn release_window(&mut self, app: &mut App) {
        self.theme_qualification_worker = None;
        self.theme_qualification = None;
        *self.appearance.borrow().navigator_admission.borrow_mut() = None;
        if let (Some(window), Some(navigator)) =
            (&self.window, self.appearance.borrow().navigator.clone())
        {
            let _ = window.window_handle().update(app, |_, window, cx| {
                navigator.update(cx, |navigator, cx| navigator.deactivate(window, cx))
            });
        }
        self.retire_theme_reads();
        self.theme_pulse = None;
        self.subscription = None;
        let Some(window) = self.window.take() else {
            return;
        };
        let _ = self
            .appearance_owner
            .update(app, |owner, _| owner.unregister(self.adapter_id));
        let _ = window
            .window_handle()
            .update(app, |_, window, _| window.remove_window());
        let receiver = window.page_split_work_receiver();
        while receiver.take_work().is_some() {}
    }
}
