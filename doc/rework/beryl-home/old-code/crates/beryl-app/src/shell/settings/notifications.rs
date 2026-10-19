use std::collections::HashMap;

use gpui_settings_window::{
    SettingsFieldId, SettingsFieldKind, SettingsRow, SettingsRowAction, SettingsRowActionId,
    SettingsSection, SettingsSectionId,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NotificationSettingsDraft {
    end_turn_sound_path: String,
}

impl NotificationSettingsDraft {
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

    #[allow(dead_code)]
    pub(crate) fn end_turn_sound_path_value(&self) -> &str {
        &self.end_turn_sound_path
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
