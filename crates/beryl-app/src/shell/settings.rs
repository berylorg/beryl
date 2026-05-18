use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
    time::Instant,
};

use gpui_settings_window::{
    RgbColor, SettingsFieldId, SettingsPageActionId, SettingsPageId, SettingsRowActionId,
    SettingsSectionId, SettingsWindowModel, SettingsWindowOptions,
};

use crate::{
    ActiveThemeProjection, AppearanceSettings, GuiPreferences, GuiPreferencesStore,
    InstalledThemeId, StylePropertyValue, StyleRoleId, ThemeDefinition, ThemeRepositorySnapshot,
    ThemeRepositoryStore,
};

#[path = "settings/developer_instructions.rs"]
mod developer_instructions;
#[path = "settings/notifications.rs"]
mod notifications;
#[path = "settings/operations.rs"]
mod operations;
#[path = "settings/theme.rs"]
mod theme;
#[path = "settings/theme_editor.rs"]
mod theme_editor;
#[path = "settings/themes.rs"]
mod themes;

use developer_instructions::{AgentSettingsDraft, developer_instructions_field_id};
use notifications::{
    NotificationSettingsDraft, NotificationSettingsRowAction, end_turn_sound_field_id,
};
use operations::OperationSettingsDraft;
use theme::settings_window_theme;

#[cfg_attr(test, allow(unused_imports))]
pub(super) use theme_editor::ThemeRoleNavigatorBodyRenderer;

pub(super) type SharedActiveThemeProjection = Arc<Mutex<ActiveThemeProjection>>;
pub(super) type SharedGuiPreferences = Arc<Mutex<GuiPreferences>>;

const SETTINGS_TEXT_INPUT_UNDO_BYTE_LIMIT: usize = 1024 * 1024;

pub(super) fn load_initial_theme_repository_snapshot(
    store: Option<&ThemeRepositoryStore>,
) -> ThemeRepositorySnapshot {
    store
        .and_then(|store| store.load_or_default().ok())
        .unwrap_or_else(ThemeRepositorySnapshot::built_in)
}

pub(super) fn load_initial_gui_preferences(store: &GuiPreferencesStore) -> GuiPreferences {
    store.load_or_default().unwrap_or_default()
}

#[derive(Clone)]
struct SettingsSaveSnapshot {
    preferences: GuiPreferences,
}

#[derive(Clone)]
struct SettingsWindowOptionsCache {
    active_style_revision: u64,
    options: SettingsWindowOptions,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ThemeEditorDiagnosticsSnapshot {
    pub(super) candidate_definition_build_count: u64,
    pub(super) last_candidate_definition_build_micros: u64,
    pub(super) preview_projection_build_count: u64,
    pub(super) last_preview_projection_build_micros: u64,
    pub(super) role_preview_style_build_count: u64,
    pub(super) total_schema_role_count: usize,
    pub(super) navigator_column_count: usize,
    pub(super) selected_role_path_count: usize,
    pub(super) selected_property_detail_row_count: usize,
    pub(super) modified_state_recompute_count: u64,
    pub(super) last_modified_state_recompute_micros: u64,
}

#[derive(Debug, Default)]
struct ThemeEditorDiagnostics {
    has_page_model_sample: bool,
    candidate_definition_build_count: u64,
    last_candidate_definition_build_micros: u64,
    preview_projection_build_count: u64,
    last_preview_projection_build_micros: u64,
    role_preview_style_build_count: u64,
    total_schema_role_count: usize,
    navigator_column_count: usize,
    selected_role_path_count: usize,
    selected_property_detail_row_count: usize,
    modified_state_recompute_count: u64,
    last_modified_state_recompute_micros: u64,
}

pub(super) struct SettingsState {
    active_theme: SharedActiveThemeProjection,
    active_preferences: SharedGuiPreferences,
    preferences_store: Option<GuiPreferencesStore>,
    theme_repository_store: Option<ThemeRepositoryStore>,
    theme_repository_snapshot: ThemeRepositorySnapshot,
    store_unavailable_message: Option<String>,
    theme_editor_draft: theme_editor::ThemeEditorDraft,
    theme_draft_modified: bool,
    selected_theme_role_id: StyleRoleId,
    theme_save_as_name: String,
    notification_draft: NotificationSettingsDraft,
    agent_draft: AgentSettingsDraft,
    operation_draft: OperationSettingsDraft,
    selected_section_id: SettingsSectionId,
    selected_page_id: SettingsPageId,
    errors: HashMap<SettingsFieldId, String>,
    save_receiver: Option<Receiver<Result<(), String>>>,
    queued_save: Option<SettingsSaveSnapshot>,
    window_options_cache: Option<SettingsWindowOptionsCache>,
    last_synced_window_options: Option<SettingsWindowOptions>,
    theme_editor_diagnostics: RefCell<ThemeEditorDiagnostics>,
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
    ActiveThemeChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsPageActionOutcome {
    Updated,
    ActiveThemeChanged,
}

impl SettingsState {
    #[allow(dead_code)]
    pub(super) fn new_with_stores(
        active_theme: SharedActiveThemeProjection,
        active_preferences: SharedGuiPreferences,
        preferences_store: GuiPreferencesStore,
    ) -> Self {
        Self::new_with_optional_stores(
            active_theme,
            active_preferences,
            Some(preferences_store),
            None,
            ThemeRepositorySnapshot::built_in(),
            None,
        )
    }

    pub(super) fn new_with_theme_repository(
        active_theme: SharedActiveThemeProjection,
        active_preferences: SharedGuiPreferences,
        preferences_store: GuiPreferencesStore,
        theme_repository_store: ThemeRepositoryStore,
        theme_repository_snapshot: ThemeRepositorySnapshot,
    ) -> Self {
        Self::new_with_optional_stores(
            active_theme,
            active_preferences,
            Some(preferences_store),
            Some(theme_repository_store),
            theme_repository_snapshot,
            None,
        )
    }

    pub(super) fn new_without_stores(
        active_theme: SharedActiveThemeProjection,
        active_preferences: SharedGuiPreferences,
        store_unavailable_message: String,
    ) -> Self {
        Self::new_with_optional_stores(
            active_theme,
            active_preferences,
            None,
            None,
            ThemeRepositorySnapshot::built_in(),
            Some(store_unavailable_message),
        )
    }

    fn new_with_optional_stores(
        active_theme: SharedActiveThemeProjection,
        active_preferences: SharedGuiPreferences,
        preferences_store: Option<GuiPreferencesStore>,
        theme_repository_store: Option<ThemeRepositoryStore>,
        theme_repository_snapshot: ThemeRepositorySnapshot,
        store_unavailable_message: Option<String>,
    ) -> Self {
        let appearance_settings =
            AppearanceSettings::from_active_theme(theme_repository_snapshot.active_projection());
        let theme_editor_draft = theme_editor::ThemeEditorDraft::from_definition(
            theme_repository_snapshot.active_definition(),
        );
        let gui_preferences = active_preferences
            .lock()
            .map(|preferences| preferences.clone())
            .unwrap_or_default();

        Self {
            active_theme,
            active_preferences,
            preferences_store,
            theme_repository_store,
            theme_repository_snapshot,
            store_unavailable_message,
            theme_editor_draft,
            theme_draft_modified: false,
            selected_theme_role_id: theme_editor::default_role_id(),
            theme_save_as_name: default_save_as_name(&appearance_settings),
            notification_draft: NotificationSettingsDraft::from_preferences(
                &gui_preferences.notifications,
            ),
            agent_draft: AgentSettingsDraft::from_preferences(&gui_preferences.agent),
            operation_draft: OperationSettingsDraft::from_preferences(&gui_preferences.operations),
            selected_section_id: themes::section_id(),
            selected_page_id: themes::root_page_id(),
            errors: HashMap::new(),
            save_receiver: None,
            queued_save: None,
            window_options_cache: None,
            last_synced_window_options: None,
            theme_editor_diagnostics: RefCell::new(ThemeEditorDiagnostics::default()),
        }
    }

    pub(super) fn window_options(&mut self) -> SettingsWindowOptions {
        let active = self.active_theme_snapshot();
        let active_style_revision = active.style_revision();
        if let Some(cache) = &self.window_options_cache
            && cache.active_style_revision == active_style_revision
        {
            return cache.options.clone();
        }

        let options = Self::window_options_for_active_theme(&active);
        self.window_options_cache = Some(SettingsWindowOptionsCache {
            active_style_revision,
            options: options.clone(),
        });
        options
    }

    pub(super) fn window_options_for_sync(&mut self) -> Option<SettingsWindowOptions> {
        let options = self.window_options();
        if self.last_synced_window_options.as_ref() == Some(&options) {
            None
        } else {
            Some(options)
        }
    }

    pub(super) fn record_window_options_synced(&mut self, options: SettingsWindowOptions) {
        self.last_synced_window_options = Some(options);
    }

    pub(super) fn reset_draft_from_active(&mut self) {
        self.theme_editor_draft = theme_editor::ThemeEditorDraft::from_definition(
            self.theme_repository_snapshot.active_definition(),
        );
        self.theme_draft_modified = false;
        self.reconcile_selected_theme_role_id();
        let settings = AppearanceSettings::from_active_theme(
            self.theme_repository_snapshot.active_projection(),
        );
        self.theme_save_as_name = default_save_as_name(&settings);
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

    pub(super) fn active_preferences_snapshot(&self) -> GuiPreferences {
        self.active_preferences
            .lock()
            .map(|preferences| preferences.clone())
            .unwrap_or_default()
    }

    pub(super) fn apply_preferences_from_external(
        &mut self,
        next: GuiPreferences,
    ) -> Result<bool, String> {
        if self.preference_draft_modified() {
            return Err(
                "The settings window has unapplied settings drafts. Apply, cancel, or reset them before CAS settings writes."
                    .to_string(),
            );
        }
        let current = self.active_preferences_snapshot();
        let changed = next != current;
        if !changed {
            return Ok(false);
        }
        self.commit_gui_preferences(next.clone());
        Ok(changed)
    }

    pub(super) fn select_section(&mut self, section_id: SettingsSectionId) {
        if themes::has_section_id(&section_id)
            || notifications::has_section_id(&section_id)
            || developer_instructions::has_section_id(&section_id)
            || operations::has_section_id(&section_id)
        {
            self.selected_section_id = section_id.clone();
            self.selected_page_id = SettingsPageId::from(section_id.as_str().to_string());
        }
    }

    pub(super) fn select_page(&mut self, page_id: SettingsPageId) {
        if themes::has_page_id(&page_id) {
            self.selected_section_id = themes::section_id();
            self.selected_page_id = page_id;
        }
    }

    pub(super) fn select_theme_editor_role_id(&mut self, role_id: StyleRoleId) {
        let role_id = theme_editor::validated_role_id(role_id);
        self.selected_section_id = themes::section_id();
        self.selected_page_id = themes::editor_page_id();
        self.selected_theme_role_id = role_id;
    }

    pub(super) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) {
        if *field_id == themes::save_as_name_field_id() {
            self.theme_save_as_name = value;
            self.errors.remove(field_id);
            return;
        }
        if self
            .theme_editor_draft
            .set_field_value(field_id, value.clone())
        {
            self.theme_draft_modified = true;
            self.errors.remove(field_id);
            return;
        }
        if self
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
        if let Some(action) = themes::row_action(field_id, action_id) {
            return self.handle_theme_row_action(action);
        }
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

    pub(super) fn handle_page_action(
        &mut self,
        action_id: &SettingsPageActionId,
    ) -> Option<SettingsPageActionOutcome> {
        match themes::page_action(action_id)? {
            themes::ThemePageAction::Save => Some(self.save_active_theme()),
            themes::ThemePageAction::SaveAs => Some(self.save_active_theme_as()),
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
        let theme_editor_model = (self.selected_page_id == themes::editor_page_id()).then(|| {
            let model = self
                .theme_editor_draft
                .page_model(&self.selected_theme_role_id, &self.errors);
            self.theme_editor_diagnostics
                .borrow_mut()
                .record_page_model(model.diagnostics);
            model
        });
        let mut sections = vec![themes::settings_section(
            &self.theme_repository_snapshot,
            theme_editor_model,
            &self.errors,
            self.theme_draft_modified(),
            &self.theme_save_as_name,
        )];
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

        SettingsWindowModel::with_selected_page(
            sections,
            self.selected_section_id.clone(),
            self.selected_page_id.clone(),
        )
        .expect("Beryl settings model uses static unique sections and fields")
    }

    pub(super) fn theme_editor_diagnostics_snapshot(
        &self,
    ) -> Option<ThemeEditorDiagnosticsSnapshot> {
        if self.selected_page_id != themes::editor_page_id() {
            return None;
        }
        self.theme_editor_diagnostics.borrow().snapshot()
    }

    pub(super) fn theme_editor_role_tree_projection(
        &self,
    ) -> theme_editor::ThemeRoleNavigatorProjection {
        self.theme_editor_draft
            .page_model(&self.selected_theme_role_id, &self.errors)
            .role_tree
    }

    pub(super) fn selected_theme_editor_role_tree_projection(
        &self,
    ) -> Option<theme_editor::ThemeRoleNavigatorProjection> {
        (self.selected_page_id == themes::editor_page_id())
            .then(|| self.theme_editor_role_tree_projection())
    }

    pub(super) fn theme_editor_role_navigator_body_renderer(
        on_select_role: impl Fn(StyleRoleId, &mut gpui::App) + 'static,
    ) -> theme_editor::ThemeRoleNavigatorBodyRenderer {
        theme_editor::ThemeRoleNavigatorBodyRenderer::new(on_select_role)
    }

    #[cfg(test)]
    pub(super) fn theme_role_navigator_render_strategy_for_test()
    -> theme_editor::ThemeRoleNavigatorRenderStrategy {
        theme_editor::theme_role_navigator_render_strategy_for_test()
    }

    #[cfg(test)]
    pub(super) fn theme_role_navigator_row_window_for_test(
        row_count: usize,
        scroll_offset: f32,
        viewport_height: f32,
    ) -> std::ops::Range<usize> {
        theme_editor::theme_role_navigator_row_window_for_test(
            row_count,
            scroll_offset,
            viewport_height,
        )
    }

    #[cfg(test)]
    pub(super) fn theme_role_navigator_row_window_height_sum_for_test(
        row_count: usize,
        scroll_offset: f32,
        viewport_height: f32,
    ) -> (std::ops::Range<usize>, f32, f32) {
        theme_editor::theme_role_navigator_row_window_height_sum_for_test(
            row_count,
            scroll_offset,
            viewport_height,
        )
    }

    #[cfg(test)]
    pub(super) fn selected_theme_role_id(&self) -> &StyleRoleId {
        &self.selected_theme_role_id
    }

    #[cfg(test)]
    pub(super) fn set_selected_theme_role_id_for_test(&mut self, role_id: StyleRoleId) {
        self.selected_theme_role_id = role_id;
    }

    pub(super) fn apply(&mut self) -> bool {
        let mut errors = HashMap::new();
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

        self.commit_gui_preferences(gui_preferences);

        true
    }

    pub(super) fn has_pending_save(&self) -> bool {
        self.save_receiver.is_some() || self.queued_save.is_some()
    }

    fn commit_gui_preferences(&mut self, gui_preferences: GuiPreferences) {
        self.notification_draft =
            NotificationSettingsDraft::from_preferences(&gui_preferences.notifications);
        self.agent_draft = AgentSettingsDraft::from_preferences(&gui_preferences.agent);
        self.operation_draft =
            OperationSettingsDraft::from_preferences(&gui_preferences.operations);
        self.errors.clear();

        if let Ok(mut active) = self.active_preferences.lock() {
            *active = gui_preferences.clone();
        }

        self.enqueue_save(SettingsSaveSnapshot {
            preferences: gui_preferences,
        });
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
        let Some(preferences_store) = self.preferences_store.clone() else {
            let message = self.store_unavailable_message.clone().unwrap_or_else(|| {
                "Beryl settings storage is unavailable for the configured home directory."
                    .to_string()
            });
            let _ = sender.send(Err(message));
            self.save_receiver = Some(receiver);
            return;
        };

        thread::spawn(move || {
            let result = preferences_store
                .save(&snapshot.preferences)
                .map_err(|error| error.to_string());
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

    pub(super) fn theme_repository_snapshot(&self) -> &ThemeRepositorySnapshot {
        &self.theme_repository_snapshot
    }

    pub(super) fn record_theme_repository_snapshot(&mut self, snapshot: ThemeRepositorySnapshot) {
        self.theme_repository_snapshot = snapshot;
        self.theme_draft_modified = self.compute_theme_draft_modified();
    }

    pub(super) fn apply_theme_repository_snapshot_from_external(
        &mut self,
        snapshot: ThemeRepositorySnapshot,
    ) {
        self.apply_theme_repository_snapshot(snapshot);
    }

    pub(super) fn theme_draft_modified_for_external_change(&self) -> bool {
        self.theme_draft_modified()
    }

    fn active_theme_snapshot(&self) -> ActiveThemeProjection {
        self.active_theme
            .lock()
            .map(|theme| theme.clone())
            .unwrap_or_else(|_| self.theme_repository_snapshot.active_projection().clone())
    }

    fn window_options_for_active_theme(active: &ActiveThemeProjection) -> SettingsWindowOptions {
        SettingsWindowOptions::new("Beryl Settings")
            .with_saved_color_swatches(Self::saved_color_swatches_for_active_theme(active))
            .with_text_input_undo_byte_limit(SETTINGS_TEXT_INPUT_UNDO_BYTE_LIMIT)
            .with_visual_theme(settings_window_theme(active))
    }

    fn saved_color_swatches_for_active_theme(active: &ActiveThemeProjection) -> Vec<RgbColor> {
        let built_in = ActiveThemeProjection::built_in();
        let mut seen = BTreeSet::new();
        let mut colors = Vec::new();

        for theme in [active, &built_in] {
            for style in theme.default_styles().values() {
                for value in style.properties().values() {
                    if let StylePropertyValue::Color(value) = value
                        && let Some(color) = RgbColor::parse(value)
                    {
                        let canonical = color.to_hex();
                        if seen.insert(canonical) {
                            colors.push(color);
                        }
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

    fn handle_theme_row_action(
        &mut self,
        action: themes::ThemeRowAction,
    ) -> Option<SettingsRowActionOutcome> {
        match action {
            themes::ThemeRowAction::Activate(id) => self
                .activate_theme(&id)
                .map(|()| SettingsRowActionOutcome::ActiveThemeChanged),
            themes::ThemeRowAction::Save => {
                Some(row_outcome_from_page_outcome(self.save_active_theme()))
            }
            themes::ThemeRowAction::SaveAs => {
                Some(row_outcome_from_page_outcome(self.save_active_theme_as()))
            }
        }
    }

    fn activate_theme(&mut self, id: &InstalledThemeId) -> Option<()> {
        if self.theme_draft_modified() {
            self.errors.insert(
                themes::save_as_name_field_id(),
                "Save or discard staged theme changes before activating another theme.".to_string(),
            );
            return None;
        }
        let store = self.theme_repository_store.as_ref()?;
        let snapshot = store.activate_theme(id).ok()?;
        self.apply_theme_repository_snapshot(snapshot);
        Some(())
    }

    fn save_active_theme(&mut self) -> SettingsPageActionOutcome {
        let Some(store) = self.theme_repository_store.clone() else {
            self.errors.insert(
                themes::save_as_name_field_id(),
                "Beryl theme storage is unavailable for the configured home directory.".to_string(),
            );
            return SettingsPageActionOutcome::Updated;
        };
        let active_id = self.theme_repository_snapshot.active_theme_id().clone();
        let definition = match self.theme_editor_definition() {
            Some(definition) => definition,
            None => return SettingsPageActionOutcome::Updated,
        };
        match store.update_theme(&active_id, definition) {
            Ok(snapshot) => {
                self.apply_theme_repository_snapshot(snapshot);
                SettingsPageActionOutcome::ActiveThemeChanged
            }
            Err(error) => {
                self.errors
                    .insert(themes::save_as_name_field_id(), error.to_string());
                SettingsPageActionOutcome::Updated
            }
        }
    }

    fn save_active_theme_as(&mut self) -> SettingsPageActionOutcome {
        let Some(store) = self.theme_repository_store.clone() else {
            self.errors.insert(
                themes::save_as_name_field_id(),
                "Beryl theme storage is unavailable for the configured home directory.".to_string(),
            );
            return SettingsPageActionOutcome::Updated;
        };
        let definition = match self.theme_editor_definition() {
            Some(definition) => definition,
            None => return SettingsPageActionOutcome::Updated,
        };
        match store.save_as_theme(&self.theme_save_as_name, definition) {
            Ok(snapshot) => {
                self.apply_theme_repository_snapshot(snapshot);
                SettingsPageActionOutcome::ActiveThemeChanged
            }
            Err(error) => {
                self.errors
                    .insert(themes::save_as_name_field_id(), error.to_string());
                SettingsPageActionOutcome::Updated
            }
        }
    }

    fn theme_editor_definition(&mut self) -> Option<ThemeDefinition> {
        match self.theme_editor_draft.to_definition() {
            Ok(definition) => {
                self.errors.retain(|field_id, _| {
                    *field_id == themes::save_as_name_field_id()
                        || !theme_editor::is_theme_editor_field_id(field_id)
                });
                Some(definition)
            }
            Err(errors) => {
                self.errors.extend(errors);
                None
            }
        }
    }

    fn apply_theme_repository_snapshot(&mut self, snapshot: ThemeRepositorySnapshot) {
        if let Ok(mut active) = self.active_theme.lock() {
            *active = snapshot.active_projection().clone();
        }
        let settings = AppearanceSettings::from_active_theme(snapshot.active_projection());
        self.theme_editor_draft =
            theme_editor::ThemeEditorDraft::from_definition(snapshot.active_definition());
        self.theme_draft_modified = false;
        self.reconcile_selected_theme_role_id();
        self.theme_save_as_name = default_save_as_name(&settings);
        self.theme_repository_snapshot = snapshot;
        self.errors.clear();
    }

    fn theme_draft_modified(&self) -> bool {
        self.theme_draft_modified
    }

    fn reconcile_selected_theme_role_id(&mut self) {
        self.selected_theme_role_id =
            theme_editor::validated_role_id(self.selected_theme_role_id.clone());
    }

    fn compute_theme_draft_modified(&self) -> bool {
        let started = Instant::now();
        let modified = self
            .theme_editor_draft
            .is_modified_from(self.theme_repository_snapshot.active_definition());
        self.theme_editor_diagnostics
            .borrow_mut()
            .record_modified_state_recompute(started.elapsed().as_micros());
        modified
    }

    fn preference_draft_modified(&self) -> bool {
        let active = self.active_preferences_snapshot();
        self.notification_draft
            != NotificationSettingsDraft::from_preferences(&active.notifications)
            || self.agent_draft != AgentSettingsDraft::from_preferences(&active.agent)
            || self.operation_draft != OperationSettingsDraft::from_preferences(&active.operations)
    }
}

impl ThemeEditorDiagnostics {
    fn record_page_model(&mut self, diagnostics: theme_editor::ThemeEditorPageModelDiagnostics) {
        self.has_page_model_sample = true;
        self.candidate_definition_build_count = self
            .candidate_definition_build_count
            .saturating_add(diagnostics.candidate_definition_build_count);
        self.last_candidate_definition_build_micros =
            diagnostics.last_candidate_definition_build_micros;
        self.preview_projection_build_count = self
            .preview_projection_build_count
            .saturating_add(diagnostics.preview_projection_build_count);
        self.last_preview_projection_build_micros =
            diagnostics.last_preview_projection_build_micros;
        self.role_preview_style_build_count = self
            .role_preview_style_build_count
            .saturating_add(diagnostics.role_preview_style_build_count);
        self.total_schema_role_count = diagnostics.total_schema_role_count;
        self.navigator_column_count = diagnostics.navigator_column_count;
        self.selected_role_path_count = diagnostics.selected_role_path_count;
        self.selected_property_detail_row_count = diagnostics.selected_property_detail_row_count;
    }

    fn record_modified_state_recompute(&mut self, micros: u128) {
        self.modified_state_recompute_count = self.modified_state_recompute_count.saturating_add(1);
        self.last_modified_state_recompute_micros = micros.min(u128::from(u64::MAX)) as u64;
    }

    fn snapshot(&self) -> Option<ThemeEditorDiagnosticsSnapshot> {
        if !self.has_page_model_sample {
            return None;
        }
        Some(ThemeEditorDiagnosticsSnapshot {
            candidate_definition_build_count: self.candidate_definition_build_count,
            last_candidate_definition_build_micros: self.last_candidate_definition_build_micros,
            preview_projection_build_count: self.preview_projection_build_count,
            last_preview_projection_build_micros: self.last_preview_projection_build_micros,
            role_preview_style_build_count: self.role_preview_style_build_count,
            total_schema_role_count: self.total_schema_role_count,
            navigator_column_count: self.navigator_column_count,
            selected_role_path_count: self.selected_role_path_count,
            selected_property_detail_row_count: self.selected_property_detail_row_count,
            modified_state_recompute_count: self.modified_state_recompute_count,
            last_modified_state_recompute_micros: self.last_modified_state_recompute_micros,
        })
    }
}

fn default_save_as_name(settings: &AppearanceSettings) -> String {
    let family = settings.general_ui.font_family.trim();
    if family.is_empty() {
        "Custom Theme".to_string()
    } else {
        format!("{family} Theme")
    }
}

fn row_outcome_from_page_outcome(outcome: SettingsPageActionOutcome) -> SettingsRowActionOutcome {
    match outcome {
        SettingsPageActionOutcome::Updated => SettingsRowActionOutcome::Updated,
        SettingsPageActionOutcome::ActiveThemeChanged => {
            SettingsRowActionOutcome::ActiveThemeChanged
        }
    }
}
