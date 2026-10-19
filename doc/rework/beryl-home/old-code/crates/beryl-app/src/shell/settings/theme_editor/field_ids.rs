use gpui_settings_window::SettingsFieldId;

use crate::{BerylThemeRole, StylePropertyId, StyleRoleId};

use super::helpers::{property_kind, role_is_schema_role};

const PROPERTY_FIELD_PREFIX: &str = "themes.editor.role.";
const PROPERTY_SOURCE_FIELD_SUFFIX: &str = ".source";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThemeEditorFieldTarget {
    PropertyValue {
        role_id: StyleRoleId,
        property_id: StylePropertyId,
    },
    PropertySource {
        role_id: StyleRoleId,
        property_id: StylePropertyId,
    },
}

pub(super) fn default_role_id() -> StyleRoleId {
    StyleRoleId::from(BerylThemeRole::AppWindow.id())
}

pub(super) fn root_role_id() -> StyleRoleId {
    StyleRoleId::from(BerylThemeRole::Root.id())
}

pub(super) fn validated_role_id(role_id: StyleRoleId) -> StyleRoleId {
    if role_is_schema_role(&role_id) {
        role_id
    } else {
        root_role_id()
    }
}

pub(super) fn is_theme_editor_field_id(field_id: &SettingsFieldId) -> bool {
    theme_editor_field_target(field_id).is_some()
}

pub(super) fn theme_editor_field_target(
    field_id: &SettingsFieldId,
) -> Option<ThemeEditorFieldTarget> {
    if let Some((role_id, property_id)) = parse_property_source_field_id(field_id) {
        return Some(ThemeEditorFieldTarget::PropertySource {
            role_id,
            property_id,
        });
    }
    parse_property_field_id(field_id).map(|(role_id, property_id)| {
        ThemeEditorFieldTarget::PropertyValue {
            role_id,
            property_id,
        }
    })
}

pub(super) fn property_field_id(
    role_id: &StyleRoleId,
    property_id: &StylePropertyId,
) -> SettingsFieldId {
    SettingsFieldId::from(format!(
        "{PROPERTY_FIELD_PREFIX}{}.{}",
        role_id.as_str(),
        property_id.as_str()
    ))
}

pub(super) fn role_field_id(role_id: &StyleRoleId) -> SettingsFieldId {
    SettingsFieldId::from(format!("{PROPERTY_FIELD_PREFIX}{}", role_id.as_str()))
}

fn parse_property_field_id(field_id: &SettingsFieldId) -> Option<(StyleRoleId, StylePropertyId)> {
    let suffix = field_id.as_str().strip_prefix(PROPERTY_FIELD_PREFIX)?;
    if suffix.ends_with(PROPERTY_SOURCE_FIELD_SUFFIX) {
        return None;
    }
    let (role_id, property_id) = suffix.rsplit_once('.')?;
    let role_id = StyleRoleId::from(role_id.to_string());
    let property_id = StylePropertyId::from(property_id.to_string());
    property_kind(&role_id, &property_id)?;
    Some((role_id, property_id))
}

pub(super) fn property_source_field_id(
    role_id: &StyleRoleId,
    property_id: &StylePropertyId,
) -> SettingsFieldId {
    SettingsFieldId::from(format!(
        "{PROPERTY_FIELD_PREFIX}{}.{}{}",
        role_id.as_str(),
        property_id.as_str(),
        PROPERTY_SOURCE_FIELD_SUFFIX
    ))
}

fn parse_property_source_field_id(
    field_id: &SettingsFieldId,
) -> Option<(StyleRoleId, StylePropertyId)> {
    let suffix = field_id
        .as_str()
        .strip_prefix(PROPERTY_FIELD_PREFIX)?
        .strip_suffix(PROPERTY_SOURCE_FIELD_SUFFIX)?;
    let (role_id, property_id) = suffix.rsplit_once('.')?;
    let role_id = StyleRoleId::from(role_id.to_string());
    let property_id = StylePropertyId::from(property_id.to_string());
    property_kind(&role_id, &property_id)?;
    Some((role_id, property_id))
}
