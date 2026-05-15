use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
};

use gpui_settings_window::{
    RgbColor, SettingsFieldId, SettingsRowActionId, SettingsSectionId, SettingsWindowModel,
    SettingsWindowOptions, SettingsWindowTheme,
};

use crate::{AppearanceSettings, AppearanceSettingsStore, GuiPreferences, GuiPreferencesStore};

#[path = "settings/appearance.rs"]
mod appearance;
#[path = "settings/developer_instructions.rs"]
mod developer_instructions;
#[path = "settings/notifications.rs"]
mod notifications;
#[path = "settings/operations.rs"]
mod operations;
#[path = "settings/theme.rs"]
mod theme;

use appearance::{
    AppearanceSettingsDraft, default_section_id, has_section_id, settings_color_values,
    settings_sections,
};
use developer_instructions::{AgentSettingsDraft, developer_instructions_field_id};
use notifications::{
    NotificationSettingsDraft, NotificationSettingsRowAction, end_turn_sound_field_id,
};
use operations::OperationSettingsDraft;
use theme::settings_window_theme;

pub(super) type SharedAppearanceSettings = Arc<Mutex<AppearanceSettings>>;
pub(super) type SharedGuiPreferences = Arc<Mutex<GuiPreferences>>;

const SETTINGS_TEXT_INPUT_UNDO_BYTE_LIMIT: usize = 1024 * 1024;

pub(super) fn load_initial_appearance_settings(
    store: &AppearanceSettingsStore,
) -> AppearanceSettings {
    store.load_or_default().unwrap_or_default()
}

pub(super) fn load_initial_gui_preferences(store: &GuiPreferencesStore) -> GuiPreferences {
    store.load_or_default().unwrap_or_default()
}

#[derive(Clone)]
struct SettingsSaveSnapshot {
    appearance: AppearanceSettings,
    preferences: GuiPreferences,
}

pub(super) struct SettingsState {
    active_appearance: SharedAppearanceSettings,
    appearance_store: Option<AppearanceSettingsStore>,
    active_preferences: SharedGuiPreferences,
    preferences_store: Option<GuiPreferencesStore>,
    store_unavailable_message: Option<String>,
    appearance_draft: AppearanceSettingsDraft,
    notification_draft: NotificationSettingsDraft,
    agent_draft: AgentSettingsDraft,
    operation_draft: OperationSettingsDraft,
    selected_section_id: SettingsSectionId,
    errors: HashMap<SettingsFieldId, String>,
    save_receiver: Option<Receiver<Result<(), String>>>,
    queued_save: Option<SettingsSaveSnapshot>,
}

pub(super) enum SettingsSavePoll {
    Idle,
    Pending,
    Saved,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsRowActionOutcome {
    PromptForEndTurnSoundPath,
    Updated,
}

impl SettingsState {
    pub(super) fn new_with_stores(
        active_appearance: SharedAppearanceSettings,
        appearance_store: AppearanceSettingsStore,
        active_preferences: SharedGuiPreferences,
        preferences_store: GuiPreferencesStore,
    ) -> Self {
        Self::new_with_optional_stores(
            active_appearance,
            Some(appearance_store),
            active_preferences,
            Some(preferences_store),
            None,
        )
    }

    pub(super) fn new_without_stores(
        active_appearance: SharedAppearanceSettings,
        active_preferences: SharedGuiPreferences,
        store_unavailable_message: String,
    ) -> Self {
        Self::new_with_optional_stores(
            active_appearance,
            None,
            active_preferences,
            None,
            Some(store_unavailable_message),
        )
    }

    fn new_with_optional_stores(
        active_appearance: SharedAppearanceSettings,
        appearance_store: Option<AppearanceSettingsStore>,
        active_preferences: SharedGuiPreferences,
        preferences_store: Option<GuiPreferencesStore>,
        store_unavailable_message: Option<String>,
    ) -> Self {
        let appearance_settings = active_appearance
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default();
        let gui_preferences = active_preferences
            .lock()
            .map(|preferences| preferences.clone())
            .unwrap_or_default();

        Self {
            active_appearance,
            appearance_store,
            active_preferences,
            preferences_store,
            store_unavailable_message,
            appearance_draft: AppearanceSettingsDraft::from_settings(&appearance_settings),
            notification_draft: NotificationSettingsDraft::from_preferences(
                &gui_preferences.notifications,
            ),
            agent_draft: AgentSettingsDraft::from_preferences(&gui_preferences.agent),
            operation_draft: OperationSettingsDraft::from_preferences(&gui_preferences.operations),
            selected_section_id: default_section_id(),
            errors: HashMap::new(),
            save_receiver: None,
            queued_save: None,
        }
    }

    pub(super) fn window_options(&self) -> SettingsWindowOptions {
        SettingsWindowOptions::new("Beryl Settings")
            .with_window_size(720.0, 760.0)
            .with_min_window_size(520.0, 420.0)
            .with_saved_color_swatches(self.saved_color_swatches())
            .with_text_input_undo_byte_limit(SETTINGS_TEXT_INPUT_UNDO_BYTE_LIMIT)
            .with_visual_theme(self.visual_theme())
    }

    pub(super) fn reset_draft_from_active(&mut self) {
        if let Ok(settings) = self
            .active_appearance
            .lock()
            .map(|settings| settings.clone())
        {
            self.appearance_draft = AppearanceSettingsDraft::from_settings(&settings);
        }
        if let Ok(preferences) = self
            .active_preferences
            .lock()
            .map(|preferences| preferences.clone())
        {
            self.notification_draft =
                NotificationSettingsDraft::from_preferences(&preferences.notifications);
            self.agent_draft = AgentSettingsDraft::from_preferences(&preferences.agent);
            self.operation_draft =
                OperationSettingsDraft::from_preferences(&preferences.operations);
        }
        self.errors.clear();
    }

    pub(super) fn select_section(&mut self, section_id: SettingsSectionId) {
        if has_section_id(&section_id)
            || notifications::has_section_id(&section_id)
            || developer_instructions::has_section_id(&section_id)
            || operations::has_section_id(&section_id)
        {
            self.selected_section_id = section_id;
        }
    }

    pub(super) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) {
        if self
            .appearance_draft
            .set_field_value(field_id, value.clone())
            || self
                .notification_draft
                .set_field_value(field_id, value.clone())
            || self.agent_draft.set_field_value(field_id, value.clone())
            || self.operation_draft.set_field_value(field_id, value)
        {
            self.errors.remove(field_id);
        }
    }

    #[allow(dead_code)]
    pub(super) fn set_notification_end_turn_sound_path(&mut self, value: String) {
        self.notification_draft.set_end_turn_sound_path(value);
        self.errors.remove(&end_turn_sound_field_id());
    }

    #[allow(dead_code)]
    pub(super) fn notification_end_turn_sound_path_value(&self) -> &str {
        self.notification_draft.end_turn_sound_path_value()
    }

    #[allow(dead_code)]
    pub(super) fn set_developer_instructions(&mut self, value: String) {
        self.agent_draft.set_developer_instructions(value);
        self.errors.remove(&developer_instructions_field_id());
    }

    #[allow(dead_code)]
    pub(super) fn developer_instructions_value(&self) -> &str {
        self.agent_draft.developer_instructions_value()
    }

    #[allow(dead_code)]
    pub(super) fn field_error(&self, field_id: &SettingsFieldId) -> Option<&str> {
        self.errors.get(field_id).map(String::as_str)
    }

    #[allow(dead_code)]
    pub(super) fn notification_end_turn_sound_field_id(&self) -> SettingsFieldId {
        end_turn_sound_field_id()
    }

    #[allow(dead_code)]
    pub(super) fn developer_instructions_field_id(&self) -> SettingsFieldId {
        developer_instructions_field_id()
    }

    pub(super) fn handle_row_action(
        &mut self,
        field_id: &SettingsFieldId,
        action_id: &SettingsRowActionId,
    ) -> Option<SettingsRowActionOutcome> {
        match notifications::row_action(field_id, action_id)? {
            NotificationSettingsRowAction::ChooseEndTurnSound => {
                Some(SettingsRowActionOutcome::PromptForEndTurnSoundPath)
            }
            NotificationSettingsRowAction::ClearEndTurnSound => {
                self.clear_notification_end_turn_sound_path();
                Some(SettingsRowActionOutcome::Updated)
            }
        }
    }

    pub(super) fn stage_notification_end_turn_sound_path_from_picker(&mut self, path: PathBuf) {
        let field_id = end_turn_sound_field_id();
        self.notification_draft
            .set_end_turn_sound_path_from_picker(path.clone());
        match notifications::validate_picked_end_turn_sound_path(&path) {
            Ok(()) => {
                self.errors.remove(&field_id);
            }
            Err(error) => {
                self.errors.insert(field_id, error);
            }
        }
    }

    pub(super) fn set_notification_end_turn_sound_picker_error(&mut self, error: String) {
        self.errors.insert(end_turn_sound_field_id(), error);
    }

    pub(super) fn model(&self) -> SettingsWindowModel {
        let mut sections = settings_sections(&self.appearance_draft, &self.errors);
        sections.push(operations::settings_section(
            &self.operation_draft,
            &self.errors,
        ));
        sections.push(notifications::settings_section(
            &self.notification_draft,
            &self.errors,
        ));
        sections.push(developer_instructions::settings_section(
            &self.agent_draft,
            &self.errors,
        ));

        SettingsWindowModel::with_selected_section(sections, self.selected_section_id.clone())
            .expect("Beryl settings model uses static unique sections and fields")
    }

    pub(super) fn apply(&mut self) -> bool {
        let mut errors = HashMap::new();
        let appearance_settings = match self.appearance_draft.to_settings() {
            Ok(settings) => Some(settings),
            Err(appearance_errors) => {
                errors.extend(appearance_errors);
                None
            }
        };
        let notification_preferences = match self.notification_draft.to_preferences() {
            Ok(preferences) => Some(preferences),
            Err(notification_errors) => {
                errors.extend(notification_errors);
                None
            }
        };
        let agent_preferences = match self.agent_draft.to_preferences() {
            Ok(preferences) => Some(preferences),
            Err(agent_errors) => {
                errors.extend(agent_errors);
                None
            }
        };
        let operation_preferences = match self.operation_draft.to_preferences() {
            Ok(preferences) => Some(preferences),
            Err(operation_errors) => {
                errors.extend(operation_errors);
                None
            }
        };

        if !errors.is_empty() {
            self.errors = errors;
            return false;
        }

        let appearance_settings =
            appearance_settings.expect("appearance settings are present when validation passes");
        let notification_preferences = notification_preferences
            .expect("notification preferences are present when validation passes");
        let agent_preferences =
            agent_preferences.expect("agent preferences are present when validation passes");
        let operation_preferences = operation_preferences
            .expect("operation preferences are present when validation passes");
        let gui_preferences = GuiPreferences {
            notifications: notification_preferences,
            agent: agent_preferences,
            operations: operation_preferences,
        };

        self.appearance_draft = AppearanceSettingsDraft::from_settings(&appearance_settings);
        self.notification_draft =
            NotificationSettingsDraft::from_preferences(&gui_preferences.notifications);
        self.agent_draft = AgentSettingsDraft::from_preferences(&gui_preferences.agent);
        self.operation_draft =
            OperationSettingsDraft::from_preferences(&gui_preferences.operations);
        self.errors.clear();

        if let Ok(mut active) = self.active_appearance.lock() {
            *active = appearance_settings.clone();
        }
        if let Ok(mut active) = self.active_preferences.lock() {
            *active = gui_preferences.clone();
        }

        self.enqueue_save(SettingsSaveSnapshot {
            appearance: appearance_settings,
            preferences: gui_preferences,
        });

        true
    }

    pub(super) fn has_pending_save(&self) -> bool {
        self.save_receiver.is_some() || self.queued_save.is_some()
    }

    fn enqueue_save(&mut self, snapshot: SettingsSaveSnapshot) {
        if self.save_receiver.is_some() {
            self.queued_save = Some(snapshot);
            return;
        }

        self.spawn_save(snapshot);
    }

    fn spawn_save(&mut self, snapshot: SettingsSaveSnapshot) {
        let (sender, receiver) = mpsc::channel();
        let (Some(appearance_store), Some(preferences_store)) = (
            self.appearance_store.clone(),
            self.preferences_store.clone(),
        ) else {
            let message = self.store_unavailable_message.clone().unwrap_or_else(|| {
                "Beryl settings storage is unavailable for the configured home directory."
                    .to_string()
            });
            let _ = sender.send(Err(message));
            self.save_receiver = Some(receiver);
            return;
        };

        thread::spawn(move || {
            let appearance_result = appearance_store
                .save(&snapshot.appearance)
                .map_err(|error| error.to_string());
            let preferences_result = preferences_store
                .save(&snapshot.preferences)
                .map_err(|error| error.to_string());
            let result = match (appearance_result, preferences_result) {
                (Ok(()), Ok(())) => Ok(()),
                (Err(appearance_error), Ok(())) => Err(appearance_error),
                (Ok(()), Err(preferences_error)) => Err(preferences_error),
                (Err(appearance_error), Err(preferences_error)) => {
                    Err(format!("{appearance_error}; {preferences_error}"))
                }
            };
            let _ = sender.send(result);
        });
        self.save_receiver = Some(receiver);
    }

    pub(super) fn poll_save(&mut self) -> SettingsSavePoll {
        let Some(receiver) = self.save_receiver.as_ref() else {
            return SettingsSavePoll::Idle;
        };

        match receiver.try_recv() {
            Ok(Ok(())) => {
                self.save_receiver = None;
                if let Some(snapshot) = self.queued_save.take() {
                    self.spawn_save(snapshot);
                    SettingsSavePoll::Pending
                } else {
                    SettingsSavePoll::Saved
                }
            }
            Ok(Err(error)) => {
                self.save_receiver = None;
                if let Some(snapshot) = self.queued_save.take() {
                    self.spawn_save(snapshot);
                }
                SettingsSavePoll::Failed(error)
            }
            Err(mpsc::TryRecvError::Empty) => SettingsSavePoll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.save_receiver = None;
                if let Some(snapshot) = self.queued_save.take() {
                    self.spawn_save(snapshot);
                }
                SettingsSavePoll::Failed("saving stopped unexpectedly".to_string())
            }
        }
    }

    pub(super) fn visual_theme(&self) -> SettingsWindowTheme {
        let active = self
            .active_appearance
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default();
        settings_window_theme(&active)
    }

    fn saved_color_swatches(&self) -> Vec<RgbColor> {
        let active = self
            .active_appearance
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default();
        let defaults = AppearanceSettings::default();
        let mut seen = BTreeSet::new();
        let mut colors = Vec::new();

        for settings in [&active, &defaults] {
            for value in settings_color_values(settings) {
                if let Some(color) = RgbColor::parse(&value) {
                    let canonical = color.to_hex();
                    if seen.insert(canonical) {
                        colors.push(color);
                    }
                }
            }
        }

        colors
    }
}

impl SettingsState {
    fn clear_notification_end_turn_sound_path(&mut self) {
        self.notification_draft
            .set_end_turn_sound_path(String::new());
        self.errors.remove(&end_turn_sound_field_id());
    }
}
