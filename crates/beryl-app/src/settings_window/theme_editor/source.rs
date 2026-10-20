use std::{ops::Range, rc::Rc, sync::OnceLock};

use beryl_state::{ThemeRoleId, canonical_theme_schema};

use super::{ThemeRoleColumn, ThemeRoleProjection, ThemeRoleRow, ThemeRoleSource};

pub(in crate::settings_window) struct CanonicalRoleSource;

impl CanonicalRoleSource {
    pub(in crate::settings_window) fn projection(
        revision: u64,
        selected: &ThemeRoleId,
        property_row_count: usize,
    ) -> ThemeRoleProjection {
        let schema = canonical_theme_schema();
        let mut path = vec![selected.clone()];
        while let Some(parent) = schema
            .role(path.last().unwrap().as_str())
            .and_then(|role| role.static_parent())
        {
            path.push(parent.clone());
            assert!(
                path.len() <= schema.roles().len(),
                "canonical role hierarchy is acyclic"
            );
        }
        path.reverse();
        let mut parents = vec![None];
        parents.extend(
            path.iter()
                .filter(|id| {
                    schema
                        .roles()
                        .values()
                        .any(|role| role.static_parent() == Some(*id))
                })
                .cloned()
                .map(Some),
        );
        let columns = parents
            .into_iter()
            .enumerate()
            .map(|(index, parent)| {
                let logical_item_count = schema
                    .roles()
                    .values()
                    .filter(|role| role.static_parent() == parent.as_ref())
                    .count();
                let header = parent
                    .as_ref()
                    .map(|role| role.as_str().to_owned())
                    .unwrap_or_else(|| "Roles".to_owned());
                ThemeRoleColumn {
                    parent,
                    selected: path.get(index).cloned(),
                    logical_item_count,
                    header,
                }
            })
            .collect();
        ThemeRoleProjection {
            revision,
            selected: Some(selected.clone()),
            columns,
            source: Rc::new(Self),
            property_row_count,
        }
    }
}

impl ThemeRoleSource for CanonicalRoleSource {
    fn page(&self, parent: Option<&ThemeRoleId>, range: Range<usize>) -> Vec<ThemeRoleRow> {
        assert!(
            range.end >= range.start && range.len() <= 64,
            "role projection page is bounded"
        );
        let schema = canonical_theme_schema();
        let mut rows: Vec<_> = schema
            .roles()
            .values()
            .filter(|role| role.static_parent() == parent)
            .skip(range.start)
            .take(range.len())
            .map(|role| ThemeRoleRow {
                id: role.id().clone(),
                label: role.id().as_str().to_owned(),
                has_children: false,
                supported_property_count: role.properties().len(),
            })
            .collect();
        for role in schema.roles().values() {
            if let Some(parent) = role.static_parent() {
                if let Some(row) = rows.iter_mut().find(|row| &row.id == parent) {
                    row.has_children = true;
                }
            }
        }
        rows
    }

    fn index_of(&self, parent: Option<&ThemeRoleId>, role: &ThemeRoleId) -> Option<usize> {
        canonical_theme_schema()
            .roles()
            .values()
            .filter(|candidate| candidate.static_parent() == parent)
            .position(|candidate| candidate.id() == role)
    }

    fn total_role_count(&self) -> usize {
        canonical_theme_schema().roles().len()
    }

    fn max_column_count(&self) -> usize {
        static DEPTH: OnceLock<usize> = OnceLock::new();
        *DEPTH.get_or_init(|| {
            let schema = canonical_theme_schema();
            schema
                .roles()
                .values()
                .map(|role| {
                    let mut depth = 1;
                    let mut current = role;
                    while let Some(parent) = current.static_parent() {
                        current = schema
                            .role(parent.as_str())
                            .expect("canonical parent exists");
                        depth += 1;
                        assert!(
                            depth <= schema.roles().len(),
                            "canonical role hierarchy is acyclic"
                        );
                    }
                    depth + 1
                })
                .max()
                .unwrap_or(1)
        })
    }
}
