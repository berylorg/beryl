use gpui::ScrollHandle;
use gpui_scrollbar::{ScrollbarVisibilityPolicy, ScrollbarVisibilityState};

use crate::StyleRoleId;

use super::super::ThemeRoleNavigatorColumn;
use super::chrome::navigator_scrollbar_update_callback;

#[derive(Clone)]
pub(super) struct ThemeRoleNavigatorScrollState {
    horizontal: ScrollHandle,
    horizontal_visibility: ScrollbarVisibilityState,
    columns: Vec<ThemeRoleNavigatorColumnScrollState>,
}

#[derive(Clone)]
struct ThemeRoleNavigatorColumnScrollState {
    key: Option<StyleRoleId>,
    handle: ScrollHandle,
    visibility: ScrollbarVisibilityState,
}

impl ThemeRoleNavigatorScrollState {
    pub(super) fn new() -> Self {
        Self {
            horizontal: ScrollHandle::new(),
            horizontal_visibility: ScrollbarVisibilityState::new(),
            columns: Vec::new(),
        }
    }

    pub(super) fn clear_columns(&mut self) {
        self.columns.clear();
    }

    pub(super) fn horizontal_handle(&self) -> ScrollHandle {
        self.horizontal.clone()
    }

    pub(super) fn horizontal_visibility_policy(&self) -> ScrollbarVisibilityPolicy {
        self.horizontal_visibility
            .managed(navigator_scrollbar_update_callback())
    }

    pub(super) fn column_handle(&self, index: usize) -> Option<ScrollHandle> {
        self.columns.get(index).map(|state| state.handle.clone())
    }

    pub(super) fn column_visibility_policy(
        &self,
        index: usize,
    ) -> Option<ScrollbarVisibilityPolicy> {
        self.columns.get(index).map(|state| {
            state
                .visibility
                .managed(navigator_scrollbar_update_callback())
        })
    }

    pub(super) fn column_keys(&self) -> impl Iterator<Item = &Option<StyleRoleId>> {
        self.columns.iter().map(|state| &state.key)
    }

    pub(super) fn reconcile(&mut self, columns: &[ThemeRoleNavigatorColumn]) {
        let mut previous = std::mem::take(&mut self.columns)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        let mut next = Vec::with_capacity(columns.len());
        for (index, column) in columns.iter().enumerate() {
            let key = column.parent_role_id().cloned();
            let state = take_matching_column_state_at(&mut previous, index, &key)
                .or_else(|| take_matching_column_state(&mut previous, &key))
                .unwrap_or_else(|| ThemeRoleNavigatorColumnScrollState {
                    key: key.clone(),
                    handle: ScrollHandle::new(),
                    visibility: ScrollbarVisibilityState::new(),
                });
            next.push(ThemeRoleNavigatorColumnScrollState { key, ..state });
        }
        self.columns = next;
    }
}

fn take_matching_column_state_at(
    states: &mut [Option<ThemeRoleNavigatorColumnScrollState>],
    index: usize,
    key: &Option<StyleRoleId>,
) -> Option<ThemeRoleNavigatorColumnScrollState> {
    let slot = states.get_mut(index)?;
    if slot.as_ref().is_some_and(|state| &state.key == key) {
        return slot.take();
    }
    None
}

fn take_matching_column_state(
    states: &mut [Option<ThemeRoleNavigatorColumnScrollState>],
    key: &Option<StyleRoleId>,
) -> Option<ThemeRoleNavigatorColumnScrollState> {
    let index = states
        .iter()
        .position(|slot| slot.as_ref().is_some_and(|state| &state.key == key))?;
    states[index].take()
}
