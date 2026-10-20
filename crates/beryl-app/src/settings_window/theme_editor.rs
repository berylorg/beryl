use std::collections::BTreeMap;

use beryl_state::{
    ThemeDocument, ThemeObservedDocument, ThemePropertyId, ThemePropertySource, ThemeRoleId,
    ThemeValue, ThemeValueKind, canonical_theme_schema,
};
use gpui_settings_window::{
    SettingsChoiceOption, SettingsFieldAvailability, SettingsFieldKind, SettingsFieldValidity,
    SettingsRow, SettingsRowDetailField,
};

pub mod navigator;
pub use navigator::*;

mod controller;
mod source;
pub(super) use source::CanonicalRoleSource;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceMode {
    Concrete,
    StaticParent,
    AmbientParent,
    Fallback,
}

impl SourceMode {
    fn id(self) -> &'static str {
        match self {
            Self::Concrete => "value",
            Self::StaticParent => "static_parent",
            Self::AmbientParent => "ambient_parent",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PropertyDraft {
    pub(super) source: SourceMode,
    pub(super) value: String,
}

pub(super) struct ThemeDocumentEditor {
    pub(super) baseline: ThemeObservedDocument,
    pub(super) clean_document: ThemeDocument,
    pub(super) selected_role: ThemeRoleId,
    pub(super) revision: u64,
    pub(super) projection_revision: u64,
    pub(super) changes: BTreeMap<(ThemeRoleId, ThemePropertyId), PropertyDraft>,
    pub(super) validated: Option<ThemeDocument>,
    pub(super) validated_revision: u64,
    pub(super) validating: bool,
    pub(super) issue: Option<&'static str>,
    pub(super) feedback: Option<(String, &'static str)>,
    pub(super) save_as_name: String,
    pub(super) saved_identity: Option<beryl_state::ThemeDocumentIdentity>,
}

impl ThemeDocumentEditor {
    pub(super) fn new(
        baseline: ThemeObservedDocument,
        revision: u64,
        projection_revision: u64,
    ) -> Self {
        let selected_role = canonical_theme_schema()
            .roles()
            .values()
            .find(|role| role.static_parent().is_none())
            .expect("canonical schema has a real root")
            .id()
            .clone();
        Self {
            clean_document: baseline.document().clone(),
            validated: Some(baseline.document().clone()),
            baseline,
            selected_role,
            revision,
            projection_revision,
            changes: BTreeMap::new(),
            validated_revision: revision,
            validating: false,
            issue: None,
            feedback: None,
            save_as_name: String::new(),
            saved_identity: None,
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        !self.changes.is_empty()
    }
    pub(super) fn can_save(&self) -> bool {
        self.is_dirty()
            && !self.validating
            && self.validated_revision == self.revision
            && self.validated.is_some()
    }

    pub(super) fn property(&self, property: ThemePropertyId) -> PropertyDraft {
        self.changes
            .get(&(self.selected_role.clone(), property))
            .cloned()
            .unwrap_or_else(|| self.baseline_property(&self.selected_role, property))
    }

    fn baseline_property(&self, role: &ThemeRoleId, property: ThemePropertyId) -> PropertyDraft {
        let schema = canonical_theme_schema()
            .role(role.as_str())
            .and_then(|role| role.property(property))
            .expect("canonical supported property");
        let source = self
            .clean_document
            .definition()
            .roles()
            .get(role)
            .and_then(|role| role.properties().get(&property));
        let mode = match source {
            Some(ThemePropertySource::Concrete(_)) => SourceMode::Concrete,
            Some(ThemePropertySource::StaticParent) => SourceMode::StaticParent,
            Some(ThemePropertySource::AmbientParent) => SourceMode::AmbientParent,
            Some(ThemePropertySource::Fallback) | None => SourceMode::Fallback,
        };
        let value = match source {
            Some(ThemePropertySource::Concrete(value)) => value,
            _ => schema.fallback(),
        };
        PropertyDraft {
            source: mode,
            value: value_text(value),
        }
    }

    pub(super) fn stage(&mut self, id: &str, value: String, revision: u64) -> bool {
        let role = canonical_theme_schema()
            .role(self.selected_role.as_str())
            .expect("canonical selected role");
        for schema in role.properties().values() {
            let source_id = field_id(&self.selected_role, schema.id(), "source");
            let value_id = field_id(&self.selected_role, schema.id(), "value");
            if id != source_id && id != value_id {
                continue;
            }
            let mut property = self.property(schema.id());
            if id == source_id {
                property.source = match value.as_str() {
                    "value" => SourceMode::Concrete,
                    "static_parent" if schema.static_parent_eligible() => SourceMode::StaticParent,
                    "ambient_parent" if schema.ambient_parent_eligible() => {
                        SourceMode::AmbientParent
                    }
                    "fallback" => SourceMode::Fallback,
                    _ => return false,
                };
            } else {
                if property.source != SourceMode::Concrete || value.len() > 256 {
                    return false;
                }
                property.value = value;
            }
            let key = (self.selected_role.clone(), schema.id());
            if property == self.baseline_property(&self.selected_role, schema.id()) {
                self.changes.remove(&key);
            } else {
                self.changes.insert(key, property);
            }
            self.revision = revision;
            self.issue = None;
            self.feedback = None;
            self.validating = true;
            return true;
        }
        false
    }

    pub(super) fn rows(&self, unavailable: Option<&'static str>) -> Vec<SettingsRow> {
        let role = canonical_theme_schema()
            .role(self.selected_role.as_str())
            .expect("canonical selected role");
        role.properties()
            .values()
            .map(|schema| {
                let property = self.property(schema.id());
                let mut row = SettingsRow::new(
                    field_id(&self.selected_role, schema.id(), "source"),
                    schema.id().as_str(),
                    property.source.id(),
                    SettingsFieldKind::Choice,
                )
                .with_choice(SettingsChoiceOption::new("value", "Value"))
                .with_choice(SettingsChoiceOption::new("fallback", "Built-in fallback"))
                .with_modified(
                    self.changes
                        .contains_key(&(self.selected_role.clone(), schema.id())),
                );
                if schema.static_parent_eligible() {
                    row = row
                        .with_choice(SettingsChoiceOption::new("static_parent", "Static parent"));
                }
                if schema.ambient_parent_eligible() {
                    row = row.with_choice(SettingsChoiceOption::new(
                        "ambient_parent",
                        "Ambient parent",
                    ));
                }
                if let Some(reason) = unavailable {
                    row = row.with_availability(SettingsFieldAvailability::unavailable(reason));
                }
                if let Some(issue) = self.issue {
                    row = row
                        .with_error(issue)
                        .with_validity(SettingsFieldValidity::Invalid);
                }
                if property.source == SourceMode::Concrete {
                    let kind = match schema.kind() {
                        ThemeValueKind::Color => SettingsFieldKind::Color,
                        ThemeValueKind::FontFamily => SettingsFieldKind::Text,
                        ThemeValueKind::FontStyle | ThemeValueKind::TextDecoration => {
                            SettingsFieldKind::Choice
                        }
                        _ => SettingsFieldKind::Number,
                    };
                    let mut detail = SettingsRowDetailField::new(
                        field_id(&self.selected_role, schema.id(), "value"),
                        property.value,
                        kind,
                    );
                    if kind != SettingsFieldKind::Choice {
                        detail = detail.with_value_byte_limit(Some(256));
                    }
                    if schema.kind() == ThemeValueKind::FontStyle {
                        detail = detail
                            .with_choice(SettingsChoiceOption::new("normal", "Normal"))
                            .with_choice(SettingsChoiceOption::new("italic", "Italic"));
                    }
                    if schema.kind() == ThemeValueKind::TextDecoration {
                        detail = detail
                            .with_choice(SettingsChoiceOption::new("none", "None"))
                            .with_choice(SettingsChoiceOption::new("line-through", "Line through"));
                    }
                    if let Some(reason) = unavailable {
                        detail = detail
                            .with_availability(SettingsFieldAvailability::unavailable(reason));
                    }
                    if self.issue.is_some() {
                        detail = detail.with_validity(SettingsFieldValidity::Invalid);
                    }
                    row = row.with_detail_field(detail);
                }
                if let Some((field, feedback)) = &self.feedback {
                    if field == row.field_id().as_str()
                        || row
                            .detail_field()
                            .is_some_and(|detail| detail.field_id().as_str() == field)
                    {
                        row = row.with_error(*feedback);
                    }
                }
                row
            })
            .collect()
    }
}

fn field_id(role: &ThemeRoleId, property: ThemePropertyId, part: &str) -> String {
    format!(
        "theme.property.{}.{}.{part}",
        role.as_str(),
        property.as_str()
    )
}

fn value_text(value: &ThemeValue) -> String {
    match value {
        ThemeValue::Color(value) => value.to_string(),
        ThemeValue::FontFamily(value) => value.as_str().to_owned(),
        ThemeValue::LogicalPixels(value) => value.get().to_string(),
        ThemeValue::FontWeight(value) => value.get().to_string(),
        ThemeValue::FontScale(value) => value.get().to_string(),
        ThemeValue::FontStyle(value) => value.as_str().to_owned(),
        ThemeValue::TextDecoration(value) => value.as_str().to_owned(),
    }
}

pub(super) fn validate_changes(
    document: ThemeDocument,
    changes: BTreeMap<(ThemeRoleId, ThemePropertyId), PropertyDraft>,
) -> Result<ThemeDocument, &'static str> {
    let mut typed = BTreeMap::new();
    for ((role, property), draft) in changes {
        let schema = canonical_theme_schema()
            .role(role.as_str())
            .and_then(|role| role.property(property))
            .ok_or("This role does not support that property.")?;
        let invalid = "Enter a valid concrete value for the selected property.";
        let source = match draft.source {
            SourceMode::StaticParent => ThemePropertySource::StaticParent,
            SourceMode::AmbientParent => ThemePropertySource::AmbientParent,
            SourceMode::Fallback => ThemePropertySource::Fallback,
            SourceMode::Concrete => ThemePropertySource::Concrete(match schema.kind() {
                ThemeValueKind::Color => ThemeValue::Color(
                    beryl_state::ThemeColor::parse(&draft.value).map_err(|_| invalid)?,
                ),
                ThemeValueKind::FontFamily => ThemeValue::FontFamily(
                    beryl_state::ThemeFontFamily::new(&draft.value).map_err(|_| invalid)?,
                ),
                ThemeValueKind::LogicalPixels => ThemeValue::LogicalPixels(
                    beryl_state::ThemeLogicalPixels::new(draft.value.parse().map_err(|_| invalid)?)
                        .map_err(|_| invalid)?,
                ),
                ThemeValueKind::FontWeight => ThemeValue::FontWeight(
                    beryl_state::ThemeFontWeight::new(draft.value.parse().map_err(|_| invalid)?)
                        .map_err(|_| invalid)?,
                ),
                ThemeValueKind::FontScale => ThemeValue::FontScale(
                    beryl_state::ThemeFontScale::new(draft.value.parse().map_err(|_| invalid)?)
                        .map_err(|_| invalid)?,
                ),
                ThemeValueKind::FontStyle => ThemeValue::FontStyle(match draft.value.as_str() {
                    "normal" => beryl_state::ThemeFontStyle::Normal,
                    "italic" => beryl_state::ThemeFontStyle::Italic,
                    _ => return Err(invalid),
                }),
                ThemeValueKind::TextDecoration => {
                    ThemeValue::TextDecoration(match draft.value.as_str() {
                        "none" => beryl_state::ThemeTextDecoration::None,
                        "line-through" => beryl_state::ThemeTextDecoration::LineThrough,
                        _ => return Err(invalid),
                    })
                }
            }),
        };
        typed.insert((role, property), source);
    }
    document
        .with_property_sources(typed)
        .map_err(|_| "The document does not satisfy the theme schema or its document bounds.")
}
