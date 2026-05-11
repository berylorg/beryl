use std::collections::BTreeMap;

use gpui::ScrollHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ColumnSelectorSurface {
    GraphOverlay,
    ThreadSelector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnSelectorKeyboardIntent {
    PreviousColumn,
    NextColumn,
    PreviousRow,
    NextRow,
    Activate,
    ToggleExpansion,
}

pub(crate) fn keyboard_intent_for_keystroke(
    keystroke: &str,
) -> Option<ColumnSelectorKeyboardIntent> {
    match keystroke {
        "left" => Some(ColumnSelectorKeyboardIntent::PreviousColumn),
        "right" => Some(ColumnSelectorKeyboardIntent::NextColumn),
        "up" => Some(ColumnSelectorKeyboardIntent::PreviousRow),
        "down" => Some(ColumnSelectorKeyboardIntent::NextRow),
        "enter" => Some(ColumnSelectorKeyboardIntent::Activate),
        "space" => Some(ColumnSelectorKeyboardIntent::ToggleExpansion),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnSelectorState<ColumnKey, RowSelection, ExpansionKey> {
    columns: Vec<ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey> {
    root_key: ColumnKey,
    selection: Option<RowSelection>,
    expansion_overrides: BTreeMap<ExpansionKey, bool>,
}

impl<ColumnKey, RowSelection, ExpansionKey>
    ColumnSelectorState<ColumnKey, RowSelection, ExpansionKey>
{
    pub(crate) fn new() -> Self {
        Self {
            columns: Vec::new(),
        }
    }

    pub(crate) fn from_root(root_key: ColumnKey) -> Self {
        let mut state = Self::new();
        state.push_root(root_key);
        state
    }

    pub(crate) fn columns(&self) -> &[ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>] {
        &self.columns
    }

    pub(crate) fn columns_mut(
        &mut self,
    ) -> &mut [ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>] {
        &mut self.columns
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.columns.len()
    }

    pub(crate) fn push_root(&mut self, root_key: ColumnKey) {
        self.columns.push(ColumnSelectorColumn::new(root_key));
    }

    pub(crate) fn retain_columns(&mut self, mut retain: impl FnMut(&ColumnKey) -> bool) {
        self.columns.retain(|column| retain(column.root_key()));
    }

    pub(crate) fn truncate_columns(&mut self, len: usize) {
        self.columns.truncate(len);
    }
}

impl<ColumnKey, RowSelection, ExpansionKey> Default
    for ColumnSelectorState<ColumnKey, RowSelection, ExpansionKey>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<ColumnKey, RowSelection, ExpansionKey>
    ColumnSelectorState<ColumnKey, RowSelection, ExpansionKey>
where
    ColumnKey: Clone + PartialEq,
    RowSelection: Clone + PartialEq,
{
    pub(crate) fn select_row(
        &mut self,
        column_index: usize,
        selection: RowSelection,
        next_root: Option<ColumnKey>,
    ) -> bool {
        if column_index >= self.columns.len() {
            return false;
        }

        let previous_len = self.columns.len();
        let tail_start = column_index + 1;
        let previous_next_root = self
            .columns
            .get(tail_start)
            .map(|column| column.root_key().clone());
        let previous_tail_had_state = self.columns[tail_start..]
            .iter()
            .any(|column| column.selection.is_some() || !column.expansion_overrides.is_empty());
        let next_root_changed = previous_next_root.as_ref() != next_root.as_ref();
        let changed = self.columns[column_index].selection != Some(selection.clone());
        self.columns[column_index].selection = Some(selection);
        self.columns.truncate(column_index + 1);
        if let Some(next_root) = next_root {
            self.columns.push(ColumnSelectorColumn::new(next_root));
        }

        changed
            || self.columns.len() != previous_len
            || next_root_changed
            || previous_tail_had_state
    }
}

impl<ColumnKey, RowSelection, ExpansionKey>
    ColumnSelectorState<ColumnKey, RowSelection, ExpansionKey>
where
    ColumnKey: Clone + Eq,
    RowSelection: Clone,
    ExpansionKey: Clone,
{
    pub(crate) fn replace_trail_preserving_expansion(
        &mut self,
        trail: impl IntoIterator<Item = (ColumnKey, Option<RowSelection>)>,
    ) {
        let previous_columns = std::mem::take(&mut self.columns);
        self.columns = trail
            .into_iter()
            .map(|(root_key, selection)| {
                let mut column = previous_columns
                    .iter()
                    .find(|column| column.root_key() == &root_key)
                    .cloned()
                    .unwrap_or_else(|| ColumnSelectorColumn::new(root_key.clone()));
                column.root_key = root_key;
                column.selection = selection;
                column
            })
            .collect();
    }
}

impl<ColumnKey, RowSelection, ExpansionKey>
    ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>
{
    pub(crate) fn new(root_key: ColumnKey) -> Self {
        Self {
            root_key,
            selection: None,
            expansion_overrides: BTreeMap::new(),
        }
    }

    pub(crate) fn root_key(&self) -> &ColumnKey {
        &self.root_key
    }

    pub(crate) fn selection(&self) -> Option<&RowSelection> {
        self.selection.as_ref()
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection = None;
    }
}

impl<ColumnKey, RowSelection, ExpansionKey>
    ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>
where
    ExpansionKey: Clone + Ord,
{
    pub(crate) fn is_expanded(&self, expansion_key: &ExpansionKey, default_expanded: bool) -> bool {
        self.expansion_overrides
            .get(expansion_key)
            .copied()
            .unwrap_or(default_expanded)
    }

    pub(crate) fn toggle_expansion(
        &mut self,
        expansion_key: &ExpansionKey,
        default_expanded: bool,
    ) -> bool {
        let current = self.is_expanded(expansion_key, default_expanded);
        let next = !current;

        if next == default_expanded {
            self.expansion_overrides.remove(expansion_key).is_some()
        } else {
            self.expansion_overrides.insert(expansion_key.clone(), next) != Some(next)
        }
    }

    pub(crate) fn retain_expansion_overrides(
        &mut self,
        mut retain: impl FnMut(&ExpansionKey) -> bool,
    ) {
        self.expansion_overrides
            .retain(|expansion_key, _| retain(expansion_key));
    }
}

#[derive(Clone)]
pub(crate) struct ColumnSelectorScrollState<ColumnKey> {
    horizontal: ScrollHandle,
    columns: Vec<(ColumnKey, ScrollHandle)>,
}

impl<ColumnKey> Default for ColumnSelectorScrollState<ColumnKey> {
    fn default() -> Self {
        Self::new()
    }
}

impl<ColumnKey> ColumnSelectorScrollState<ColumnKey> {
    pub(crate) fn new() -> Self {
        Self {
            horizontal: ScrollHandle::new(),
            columns: Vec::new(),
        }
    }

    pub(crate) fn horizontal_handle(&self) -> ScrollHandle {
        self.horizontal.clone()
    }

    pub(crate) fn column_handle(&self, column_index: usize) -> Option<ScrollHandle> {
        self.columns
            .get(column_index)
            .map(|(_, handle)| handle.clone())
    }

    pub(crate) fn column_keys(&self) -> impl Iterator<Item = &ColumnKey> {
        self.columns.iter().map(|(key, _)| key)
    }
}

impl<ColumnKey> ColumnSelectorScrollState<ColumnKey>
where
    ColumnKey: Clone + Eq,
{
    pub(crate) fn reconcile<RowSelection, ExpansionKey>(
        &mut self,
        columns: &[ColumnSelectorColumn<ColumnKey, RowSelection, ExpansionKey>],
    ) {
        let mut previous_handles = std::mem::take(&mut self.columns)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        let mut next_handles = Vec::with_capacity(columns.len());
        for (index, column) in columns.iter().enumerate() {
            let handle = take_matching_handle_at(&mut previous_handles, index, column.root_key())
                .or_else(|| take_matching_handle(&mut previous_handles, column.root_key()))
                .unwrap_or_else(ScrollHandle::new);
            next_handles.push((column.root_key().clone(), handle));
        }
        self.columns = next_handles;
    }
}

fn take_matching_handle_at<ColumnKey>(
    handles: &mut [Option<(ColumnKey, ScrollHandle)>],
    index: usize,
    column_key: &ColumnKey,
) -> Option<ScrollHandle>
where
    ColumnKey: Eq,
{
    let slot = handles.get_mut(index)?;
    if slot
        .as_ref()
        .is_some_and(|(existing_key, _)| existing_key == column_key)
    {
        return slot.take().map(|(_, handle)| handle);
    }
    None
}

fn take_matching_handle<ColumnKey>(
    handles: &mut [Option<(ColumnKey, ScrollHandle)>],
    column_key: &ColumnKey,
) -> Option<ScrollHandle>
where
    ColumnKey: Eq,
{
    let index = handles.iter().position(|slot| {
        slot.as_ref()
            .is_some_and(|(existing_key, _)| existing_key == column_key)
    })?;
    handles[index].take().map(|(_, handle)| handle)
}
