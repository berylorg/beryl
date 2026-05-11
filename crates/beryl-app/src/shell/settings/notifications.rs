use std::collections::HashMap;

use std::path::PathBuf;

use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsRowAction, SettingsRowActionId,
    SettingsSection, SettingsSectionId,
};

use crate::{
    NotificationPreferences, NotificationSoundPathError, parse_notification_sound_path_text,
    validate_notification_sound_path,
};

const NOTIFICATIONS_SECTION: &str = "notifications";
const END_TURN_SOUND_FIELD: &str = "notifications.end_turn_sound_path";
const CHOOSE_END_TURN_SOUND_ACTION: &str = "choose";
const CLEAR_END_TURN_SOUND_ACTION: &str = "clear";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationSettingsRowAction {
    ChooseEndTurnSound,
    ClearEndTurnSound,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NotificationSettingsDraft {
    end_turn_sound_path: String,
}

impl NotificationSettingsDraft {
    pub(crate) fn from_preferences(preferences: &NotificationPreferences) -> Self {
        Self {
            end_turn_sound_path: preferences
                .end_turn_sound_path()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn set_field_value(&mut self, field_id: &SettingsFieldId, value: String) -> bool {
        if *field_id != end_turn_sound_field_id() {
            return false;
        }
        self.end_turn_sound_path = value;
        true
    }

    #[allow(dead_code)]
    pub(crate) fn set_end_turn_sound_path(&mut self, value: String) {
        self.end_turn_sound_path = value;
    }

    pub(crate) fn set_end_turn_sound_path_from_picker(&mut self, path: PathBuf) {
        self.end_turn_sound_path = path.display().to_string();
    }

    #[allow(dead_code)]
    pub(crate) fn end_turn_sound_path_value(&self) -> &str {
        &self.end_turn_sound_path
    }

    pub(crate) fn to_preferences(
        &self,
    ) -> Result<NotificationPreferences, HashMap<SettingsFieldId, String>> {
        match parse_notification_sound_path_text(&self.end_turn_sound_path) {
            Ok(end_turn_sound_path) => Ok(NotificationPreferences {
                end_turn_sound_path,
            }),
            Err(error) => {
                let mut errors = HashMap::new();
                errors.insert(end_turn_sound_field_id(), notification_sound_error(error));
                Err(errors)
            }
        }
    }
}

pub(crate) fn settings_section(
    draft: &NotificationSettingsDraft,
    errors: &HashMap<SettingsFieldId, String>,
) -> SettingsSection {
    let field_id = end_turn_sound_field_id();
    let row = SettingsRow::new(
        field_id.clone(),
        "End-turn sound",
        draft.end_turn_sound_path_value(),
        SettingsFieldKind::Text,
    )
    .with_action(SettingsRowAction::new(
        choose_end_turn_sound_action_id(),
        "Choose...",
    ))
    .with_action(SettingsRowAction::new(
        clear_end_turn_sound_action_id(),
        "Clear",
    ));
    SettingsSection::new(notification_section_id(), "Notifications").with_row(
        match errors.get(&field_id) {
            Some(error) => row.with_error(error.clone()),
            None => row,
        },
    )
}

pub(crate) fn notification_section_id() -> SettingsSectionId {
    SettingsSectionId::from(NOTIFICATIONS_SECTION)
}

pub(crate) fn has_section_id(section_id: &SettingsSectionId) -> bool {
    *section_id == notification_section_id()
}

pub(crate) fn end_turn_sound_field_id() -> SettingsFieldId {
    SettingsFieldId::from(END_TURN_SOUND_FIELD)
}

pub(crate) fn choose_end_turn_sound_action_id() -> SettingsRowActionId {
    SettingsRowActionId::from(CHOOSE_END_TURN_SOUND_ACTION)
}

pub(crate) fn clear_end_turn_sound_action_id() -> SettingsRowActionId {
    SettingsRowActionId::from(CLEAR_END_TURN_SOUND_ACTION)
}

pub(crate) fn row_action(
    field_id: &SettingsFieldId,
    action_id: &SettingsRowActionId,
) -> Option<NotificationSettingsRowAction> {
    if *field_id != end_turn_sound_field_id() {
        return None;
    }

    if *action_id == choose_end_turn_sound_action_id() {
        return Some(NotificationSettingsRowAction::ChooseEndTurnSound);
    }
    if *action_id == clear_end_turn_sound_action_id() {
        return Some(NotificationSettingsRowAction::ClearEndTurnSound);
    }
    None
}

pub(crate) fn validate_picked_end_turn_sound_path(path: &std::path::Path) -> Result<(), String> {
    validate_notification_sound_path(path).map_err(notification_sound_error)
}

pub(crate) fn notification_sound_error(error: NotificationSoundPathError) -> String {
    match error {
        NotificationSoundPathError::NotAbsolute => {
            "End-turn sound path must be absolute.".to_string()
        }
        NotificationSoundPathError::NotWav => {
            "End-turn sound path must point to a .wav file.".to_string()
        }
    }
}
