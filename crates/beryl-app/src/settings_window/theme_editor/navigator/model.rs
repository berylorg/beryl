use std::{ops::Range, rc::Rc};

use beryl_state::ThemeRoleId;

use crate::theme_runtime::AdapterFailureClass;

pub const THEME_ROLE_OVERSCAN: usize = 2;
pub const THEME_ROLE_ROW_HEIGHT: f32 = 34.;
pub const THEME_ROLE_ROW_STRIDE: f32 = 38.;
pub const THEME_ROLE_VIEWPORT_HEIGHT: f32 = 108.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeRoleScrollLease {
    pub(super) widget: u64,
    pub(super) mount: u64,
    pub(super) revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRoleWheelFocus {
    Outside,
    Navigator,
    Column(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRoleWheelTarget {
    Page,
    Trail,
    Column(usize),
}

pub fn theme_role_wheel_target(
    focus: ThemeRoleWheelFocus,
    x: f32,
    y: f32,
    shift: bool,
) -> ThemeRoleWheelTarget {
    if !x.is_finite()
        || !y.is_finite()
        || (x == 0. && y == 0.)
        || focus == ThemeRoleWheelFocus::Outside
    {
        return ThemeRoleWheelTarget::Page;
    }
    if shift || x.abs() > y.abs() {
        return ThemeRoleWheelTarget::Trail;
    }
    match focus {
        ThemeRoleWheelFocus::Column(index) => ThemeRoleWheelTarget::Column(index),
        _ => ThemeRoleWheelTarget::Page,
    }
}

pub trait ThemeRoleSource {
    fn page(&self, parent: Option<&ThemeRoleId>, range: Range<usize>) -> Vec<ThemeRoleRow>;
    fn index_of(&self, parent: Option<&ThemeRoleId>, role: &ThemeRoleId) -> Option<usize>;
    fn total_role_count(&self) -> usize;
    fn max_column_count(&self) -> usize;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleRow {
    pub id: ThemeRoleId,
    pub label: String,
    pub has_children: bool,
    pub supported_property_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleColumn {
    pub parent: Option<ThemeRoleId>,
    pub selected: Option<ThemeRoleId>,
    pub logical_item_count: usize,
    pub header: String,
}

#[derive(Clone)]
pub struct ThemeRoleProjection {
    pub revision: u64,
    pub selected: Option<ThemeRoleId>,
    pub columns: Vec<ThemeRoleColumn>,
    pub source: Rc<dyn ThemeRoleSource>,
    pub property_row_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeRoleFocus {
    pub revision: u64,
    pub parent: Option<ThemeRoleId>,
    pub role: ThemeRoleId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRoleNavigationKey {
    Up,
    Down,
    Home,
    End,
    Left,
    Right,
}

pub struct ThemeRoleNavigatorState {
    pub(super) projection: ThemeRoleProjection,
    pub(super) offsets: Vec<f32>,
    pub(super) focus: Option<ThemeRoleFocus>,
    pub(super) reconciliations: u64,
}

impl ThemeRoleNavigatorState {
    pub fn new(projection: ThemeRoleProjection) -> Result<Self, AdapterFailureClass> {
        Self::validate(&projection)?;
        let mut state = Self {
            offsets: vec![0.; projection.columns.len()],
            projection,
            focus: None,
            reconciliations: 0,
        };
        state.reset_new_columns(&[]);
        Ok(state)
    }

    fn validate(projection: &ThemeRoleProjection) -> Result<(), AdapterFailureClass> {
        let reject = || AdapterFailureClass::Rejected;
        if projection.columns.len() > projection.source.max_column_count()
            || projection.property_row_count > gpui_settings_window::MAX_PAGE_DETAIL_ROWS
            || projection
                .columns
                .first()
                .is_some_and(|column| column.parent.is_some())
        {
            return Err(reject());
        }
        for (index, column) in projection.columns.iter().enumerate() {
            if column.logical_item_count > projection.source.total_role_count()
                || (index > 0
                    && (column.parent.is_none()
                        || projection.columns[index - 1].selected.as_ref()
                            != column.parent.as_ref()))
            {
                return Err(reject());
            }
            if let Some(selected) = &column.selected {
                if projection
                    .source
                    .index_of(column.parent.as_ref(), selected)
                    .is_none_or(|index| index >= column.logical_item_count)
                {
                    return Err(reject());
                }
            }
        }
        if projection
            .columns
            .iter()
            .rev()
            .find_map(|column| column.selected.as_ref())
            != projection.selected.as_ref()
        {
            return Err(reject());
        }
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        projection: ThemeRoleProjection,
    ) -> Result<(), AdapterFailureClass> {
        Self::validate(&projection)?;
        if projection.revision < self.projection.revision
            || (projection.revision == self.projection.revision
                && (projection.columns != self.projection.columns
                    || projection.selected != self.projection.selected))
        {
            return Err(AdapterFailureClass::Rejected);
        }
        let old = std::mem::replace(&mut self.projection, projection);
        let offsets = std::mem::take(&mut self.offsets);
        self.offsets = self
            .projection
            .columns
            .iter()
            .map(|column| {
                old.columns
                    .iter()
                    .position(|old| old.parent == column.parent)
                    .and_then(|index| offsets.get(index))
                    .copied()
                    .unwrap_or(0.)
            })
            .collect();
        self.reset_new_columns(&old.columns);
        for index in 0..self.offsets.len() {
            self.offsets[index] = self.offsets[index].min(self.max_offset(index));
        }
        if let Some(focus) = &self.focus {
            self.focus = self
                .locate(&focus.parent, &focus.role)
                .map(|_| ThemeRoleFocus {
                    revision: self.projection.revision,
                    parent: focus.parent.clone(),
                    role: focus.role.clone(),
                });
            if self.focus.is_none() {
                self.focus = self.fallback_focus();
            }
        }
        self.reconciliations = self.reconciliations.saturating_add(1);
        Ok(())
    }

    fn reset_new_columns(&mut self, previous: &[ThemeRoleColumn]) {
        for index in 0..self.projection.columns.len() {
            let column = &self.projection.columns[index];
            if !previous.iter().any(|old| old.parent == column.parent) {
                if let Some(ordinal) = column.selected.as_ref().and_then(|role| {
                    self.projection
                        .source
                        .index_of(column.parent.as_ref(), role)
                }) {
                    self.reveal(index, ordinal);
                }
            }
        }
    }

    fn locate(&self, parent: &Option<ThemeRoleId>, role: &ThemeRoleId) -> Option<(usize, usize)> {
        let column = self
            .projection
            .columns
            .iter()
            .position(|column| &column.parent == parent)?;
        let ordinal = self.projection.source.index_of(parent.as_ref(), role)?;
        (ordinal < self.projection.columns[column].logical_item_count).then_some((column, ordinal))
    }

    pub fn logical_focus(&self) -> Option<&ThemeRoleFocus> {
        self.focus.as_ref()
    }

    pub fn selected_role(&self) -> Option<&ThemeRoleId> {
        self.projection.selected.as_ref()
    }

    pub fn focus_is_realized(&self) -> bool {
        self.focus
            .as_ref()
            .and_then(|focus| self.locate(&focus.parent, &focus.role))
            .is_some_and(|(column, ordinal)| self.realized_range(column).contains(&ordinal))
    }

    fn fallback_focus(&self) -> Option<ThemeRoleFocus> {
        let selected = self.projection.selected.as_ref().and_then(|selected| {
            self.projection
                .columns
                .iter()
                .find(|column| column.selected.as_ref() == Some(selected))
                .map(|column| (column.parent.clone(), selected.clone()))
        });
        let (parent, role) = selected.or_else(|| {
            self.projection.columns.iter().find_map(|column| {
                self.projection
                    .source
                    .page(column.parent.as_ref(), 0..column.logical_item_count.min(1))
                    .first()
                    .map(|row| (column.parent.clone(), row.id.clone()))
            })
        })?;
        Some(ThemeRoleFocus {
            revision: self.projection.revision,
            parent,
            role,
        })
    }

    pub fn enter(&mut self) -> Option<ThemeRoleFocus> {
        if self.focus.is_none() {
            self.focus = self.fallback_focus();
        }
        if let Some((column, ordinal)) = self
            .focus
            .as_ref()
            .and_then(|focus| self.locate(&focus.parent, &focus.role))
        {
            self.reveal(column, ordinal);
        }
        self.focus.clone()
    }

    pub fn accept_activation(
        &mut self,
        revision: u64,
        parent: Option<ThemeRoleId>,
        role: ThemeRoleId,
        admitted: bool,
    ) -> bool {
        if !admitted
            || revision != self.projection.revision
            || self
                .locate(&parent, &role)
                .is_none_or(|(column, ordinal)| !self.realized_range(column).contains(&ordinal))
        {
            return false;
        }
        self.focus = Some(ThemeRoleFocus {
            revision,
            parent,
            role,
        });
        true
    }

    pub fn navigate(
        &mut self,
        key: ThemeRoleNavigationKey,
        mounted: bool,
    ) -> Option<ThemeRoleFocus> {
        if !mounted {
            return None;
        }
        self.enter()?;
        let focus = self.focus.as_ref()?;
        let (mut column, mut ordinal) = self.locate(&focus.parent, &focus.role)?;
        let descriptor = &self.projection.columns[column];
        match key {
            ThemeRoleNavigationKey::Up => ordinal = ordinal.saturating_sub(1),
            ThemeRoleNavigationKey::Down => {
                ordinal = (ordinal + 1).min(descriptor.logical_item_count.saturating_sub(1))
            }
            ThemeRoleNavigationKey::Home => ordinal = 0,
            ThemeRoleNavigationKey::End => {
                ordinal = descriptor.logical_item_count.saturating_sub(1)
            }
            ThemeRoleNavigationKey::Left => {
                let parent = descriptor.parent.as_ref()?;
                column = column.checked_sub(1)?;
                ordinal = self
                    .projection
                    .source
                    .index_of(self.projection.columns[column].parent.as_ref(), parent)?;
            }
            ThemeRoleNavigationKey::Right => {
                let next = self.projection.columns.get(column + 1)?;
                if descriptor.selected.as_ref() != Some(&focus.role)
                    || next.parent.as_ref() != Some(&focus.role)
                    || next.logical_item_count == 0
                {
                    return None;
                }
                column += 1;
                ordinal = next
                    .selected
                    .as_ref()
                    .and_then(|role| self.projection.source.index_of(next.parent.as_ref(), role))
                    .unwrap_or(0);
            }
        }
        let parent = self.projection.columns[column].parent.clone();
        let row = self
            .projection
            .source
            .page(parent.as_ref(), ordinal..ordinal + 1)
            .into_iter()
            .next()?;
        self.focus = Some(ThemeRoleFocus {
            revision: self.projection.revision,
            parent,
            role: row.id,
        });
        self.reveal(column, ordinal);
        self.focus.clone()
    }

    pub fn visible_range(&self, column: usize) -> Range<usize> {
        let count = self.projection.columns[column].logical_item_count;
        let start = (self.offsets[column] / THEME_ROLE_ROW_STRIDE).floor() as usize;
        let end = ((self.offsets[column] + THEME_ROLE_VIEWPORT_HEIGHT) / THEME_ROLE_ROW_STRIDE)
            .ceil() as usize;
        start.min(count)..end.min(count)
    }

    pub fn realized_range(&self, column: usize) -> Range<usize> {
        let visible = self.visible_range(column);
        visible.start.saturating_sub(THEME_ROLE_OVERSCAN)
            ..(visible.end + THEME_ROLE_OVERSCAN)
                .min(self.projection.columns[column].logical_item_count)
    }

    pub fn realized_page(&self, column: usize) -> Vec<ThemeRoleRow> {
        let range = self.realized_range(column);
        let rows = self.projection.source.page(
            self.projection.columns[column].parent.as_ref(),
            range.clone(),
        );
        assert!(
            rows.len() <= range.len(),
            "host role page exceeds requested bounded window"
        );
        rows
    }

    pub fn offset(&self, column: usize) -> f32 {
        self.offsets[column]
    }

    pub fn set_offset(&mut self, column: usize, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let offset = offset.clamp(0., self.max_offset(column));
        let changed = self.offsets[column] != offset;
        self.offsets[column] = offset;
        changed
    }

    pub(super) fn max_offset(&self, column: usize) -> f32 {
        let count = self.projection.columns[column].logical_item_count;
        (count as f32 * THEME_ROLE_ROW_STRIDE
            - if count > 0 { 4. } else { 0. }
            - THEME_ROLE_VIEWPORT_HEIGHT)
            .max(0.)
    }

    fn reveal(&mut self, column: usize, ordinal: usize) {
        let top = ordinal as f32 * THEME_ROLE_ROW_STRIDE;
        let offset = if top < self.offsets[column] {
            top
        } else if top + THEME_ROLE_ROW_HEIGHT > self.offsets[column] + THEME_ROLE_VIEWPORT_HEIGHT {
            top + THEME_ROLE_ROW_HEIGHT - THEME_ROLE_VIEWPORT_HEIGHT
        } else {
            self.offsets[column]
        };
        self.set_offset(column, offset);
    }
}
