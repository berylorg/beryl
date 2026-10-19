use std::collections::{BTreeMap, BTreeSet, HashMap};

use gpui_settings_window::SettingsPageSplitItemPreviewStyle;

use crate::{StyleRoleId, ThemeRoleSchema, built_in_theme_schema};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemeRoleNavigatorProjection {
    root_role_id: StyleRoleId,
    selected_role_id: StyleRoleId,
    selected_path: Vec<StyleRoleId>,
    columns: Vec<ThemeRoleNavigatorColumn>,
    rows: BTreeMap<StyleRoleId, ThemeRoleNavigatorRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemeRoleNavigatorColumn {
    parent_role_id: Option<StyleRoleId>,
    rows: Vec<ThemeRoleNavigatorRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThemeRoleNavigatorRow {
    role_id: StyleRoleId,
    label: String,
    static_parent_id: Option<StyleRoleId>,
    child_role_ids: Vec<StyleRoleId>,
    property_row_count: usize,
    preview_style: Option<SettingsPageSplitItemPreviewStyle>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ThemeRoleNavigatorProjection {
    pub(crate) fn from_built_in_schema(
        selected_role_id: &StyleRoleId,
        preview_styles: &HashMap<StyleRoleId, SettingsPageSplitItemPreviewStyle>,
    ) -> Self {
        let schema = built_in_theme_schema();
        let roles_by_id = schema
            .roles()
            .iter()
            .map(|role| (role.role_id().clone(), role))
            .collect::<BTreeMap<_, _>>();
        let root_role_id = schema_root_role_id(schema.roles());
        let children_by_parent = children_by_parent(schema.roles());

        let rows = schema
            .roles()
            .iter()
            .map(|schema_role| {
                let role_id = schema_role.role_id().clone();
                let child_role_ids = children_by_parent
                    .get(&role_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|child_id| roles_by_id.contains_key(child_id))
                    .collect::<Vec<_>>();
                let row = ThemeRoleNavigatorRow {
                    label: role_label(&role_id),
                    preview_style: preview_styles.get(&role_id).cloned(),
                    role_id: role_id.clone(),
                    static_parent_id: schema_role.static_parent().cloned(),
                    child_role_ids,
                    property_row_count: schema_role.properties().len(),
                };
                (role_id, row)
            })
            .collect::<BTreeMap<_, _>>();

        let selected_role_id = if rows.contains_key(selected_role_id) {
            selected_role_id.clone()
        } else {
            root_role_id.clone()
        };
        let selected_path = selected_path(&selected_role_id, &root_role_id, &rows);
        let columns = columns_for_selected_path(&selected_path, &rows);
        Self {
            root_role_id,
            selected_role_id,
            selected_path,
            columns,
            rows,
        }
    }

    pub(crate) fn root_role_id(&self) -> &StyleRoleId {
        &self.root_role_id
    }

    pub(crate) fn selected_role_id(&self) -> &StyleRoleId {
        &self.selected_role_id
    }

    pub(crate) fn selected_path(&self) -> &[StyleRoleId] {
        &self.selected_path
    }

    pub(crate) fn columns(&self) -> &[ThemeRoleNavigatorColumn] {
        &self.columns
    }

    pub(crate) fn rows(&self) -> impl Iterator<Item = &ThemeRoleNavigatorRow> {
        self.rows.values()
    }

    pub(crate) fn row(&self, role_id: &StyleRoleId) -> Option<&ThemeRoleNavigatorRow> {
        self.rows.get(role_id)
    }

    pub(crate) fn child_column(&self, role_id: &StyleRoleId) -> Option<ThemeRoleNavigatorColumn> {
        let parent = self.row(role_id)?;
        Some(ThemeRoleNavigatorColumn {
            parent_role_id: Some(role_id.clone()),
            rows: parent
                .child_role_ids()
                .iter()
                .filter_map(|child_id| self.row(child_id).cloned())
                .collect(),
        })
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl ThemeRoleNavigatorColumn {
    pub(crate) fn parent_role_id(&self) -> Option<&StyleRoleId> {
        self.parent_role_id.as_ref()
    }

    pub(crate) fn rows(&self) -> &[ThemeRoleNavigatorRow] {
        &self.rows
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl ThemeRoleNavigatorRow {
    pub(crate) fn role_id(&self) -> &StyleRoleId {
        &self.role_id
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn static_parent_id(&self) -> Option<&StyleRoleId> {
        self.static_parent_id.as_ref()
    }

    pub(crate) fn child_role_ids(&self) -> &[StyleRoleId] {
        &self.child_role_ids
    }

    pub(crate) fn property_row_count(&self) -> usize {
        self.property_row_count
    }

    pub(crate) fn preview_style(&self) -> Option<&SettingsPageSplitItemPreviewStyle> {
        self.preview_style.as_ref()
    }
}

fn schema_root_role_id(schema_roles: &[ThemeRoleSchema]) -> StyleRoleId {
    schema_roles
        .iter()
        .find(|role| role.static_parent().is_none())
        .map(|role| role.role_id().clone())
        .expect("built-in theme schema must include a root role")
}

fn children_by_parent(schema_roles: &[ThemeRoleSchema]) -> BTreeMap<StyleRoleId, Vec<StyleRoleId>> {
    let schema_ids = schema_roles
        .iter()
        .map(|role| role.role_id().clone())
        .collect::<BTreeSet<_>>();
    let mut children_by_parent = BTreeMap::<StyleRoleId, Vec<StyleRoleId>>::new();
    for role in schema_roles {
        let Some(parent_id) = role.static_parent() else {
            continue;
        };
        if schema_ids.contains(parent_id) {
            children_by_parent
                .entry(parent_id.clone())
                .or_default()
                .push(role.role_id().clone());
        }
    }
    children_by_parent
}

fn selected_path(
    selected_role_id: &StyleRoleId,
    root_role_id: &StyleRoleId,
    rows: &BTreeMap<StyleRoleId, ThemeRoleNavigatorRow>,
) -> Vec<StyleRoleId> {
    let mut path = Vec::new();
    let mut current_id = selected_role_id.clone();
    loop {
        path.push(current_id.clone());
        if &current_id == root_role_id {
            break;
        }
        let Some(parent_id) = rows
            .get(&current_id)
            .and_then(|row| row.static_parent_id().cloned())
        else {
            return vec![root_role_id.clone()];
        };
        if path.contains(&parent_id) {
            return vec![root_role_id.clone()];
        }
        current_id = parent_id;
    }
    path.reverse();
    path
}

fn columns_for_selected_path(
    selected_path: &[StyleRoleId],
    rows: &BTreeMap<StyleRoleId, ThemeRoleNavigatorRow>,
) -> Vec<ThemeRoleNavigatorColumn> {
    let Some(root_role_id) = selected_path.first() else {
        return Vec::new();
    };
    let Some(root_row) = rows.get(root_role_id).cloned() else {
        return Vec::new();
    };
    let mut columns = vec![ThemeRoleNavigatorColumn {
        parent_role_id: None,
        rows: vec![root_row],
    }];
    for role_id in selected_path {
        let Some(parent) = rows.get(role_id) else {
            continue;
        };
        if parent.child_role_ids().is_empty() {
            continue;
        }
        columns.push(ThemeRoleNavigatorColumn {
            parent_role_id: Some(role_id.clone()),
            rows: parent
                .child_role_ids()
                .iter()
                .filter_map(|child_id| rows.get(child_id).cloned())
                .collect(),
        });
    }
    columns
}

fn role_label(role_id: &StyleRoleId) -> String {
    role_id
        .as_str()
        .split(['.', '_'])
        .filter(|word| !word.is_empty())
        .enumerate()
        .map(|(index, word)| {
            if index == 0 {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
