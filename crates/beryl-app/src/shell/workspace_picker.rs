use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use beryl_model::{
    conversation::WorkspaceConversationState,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, RuntimeMode, WorkspaceMemberId},
};
use gpui::{Bounds, Pixels, Point, px};

use super::layout;

pub(crate) type WorkspacePickerMemberPaths = HashMap<BerylWorkspaceId, Vec<String>>;

pub(crate) const WORKSPACE_PICKER_THREAD_EDIT_WAIT_NOTICE: &str =
    "Wait for the in-progress thread edit to finish before changing workspaces.";
pub(crate) const WORKSPACE_DELETE_HOLD_DURATION: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Default)]
pub(crate) struct WorkspacePickerState {
    open: bool,
    focused_column: WorkspacePickerFocusedColumn,
    rename_editor_target: Option<BerylWorkspaceId>,
    row_action_menu: Option<WorkspaceRowActionMenuOpen>,
    delete_hold: Option<WorkspaceDeleteHoldState>,
    runtime_selector_dropdown: RuntimeSelectorDropdownState,
    member_action_menu: Option<WorkspaceMemberActionMenuOpen>,
    anchor_bounds: Option<Bounds<Pixels>>,
    popup_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WorkspacePickerFocusedColumn {
    #[default]
    Workspaces,
    Members,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceRowActionMenuOpen {
    workspace_id: BerylWorkspaceId,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeSelectorDropdownState {
    open: bool,
    highlighted_index: usize,
    trigger_bounds: Option<Bounds<Pixels>>,
    dropdown_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceMemberActionMenuOpen {
    member_id: WorkspaceMemberId,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeSelectorDistroListStatus {
    NotLoaded,
    Loading,
    Loaded,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeSelectorDistroList {
    status: RuntimeSelectorDistroListStatus,
    distro_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeSelectorRow {
    HostWindows,
    WslDistro { distro_name: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorkspacePickerPopupLayout {
    pub(crate) width: Pixels,
    pub(crate) height: Pixels,
    pub(crate) workspaces_column_width: Pixels,
    pub(crate) members_column_width: Pixels,
    pub(crate) divider_width: Pixels,
    pub(crate) workspaces_list_height: Pixels,
    pub(crate) members_list_height: Pixels,
    pub(crate) runtime_selector_dropdown_height: Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceDeleteHoldSource {
    Pointer,
    Keyboard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceDeleteHoldState {
    workspace_id: BerylWorkspaceId,
    source: WorkspaceDeleteHoldSource,
    started_at: Instant,
    duration: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspacePickerTransitionBlockers {
    pub(crate) edit_rollback_work: bool,
    pub(crate) edit_replacement_work: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspacePickerTransitionPath {
    SwitchWorkspace,
    CreateWorkspace,
    DeleteWorkspace,
}

impl Default for RuntimeSelectorDistroListStatus {
    fn default() -> Self {
        Self::NotLoaded
    }
}

impl Default for RuntimeSelectorDistroList {
    fn default() -> Self {
        Self {
            status: RuntimeSelectorDistroListStatus::NotLoaded,
            distro_names: Vec::new(),
        }
    }
}

impl WorkspacePickerState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn set_focused_column(&mut self, column: WorkspacePickerFocusedColumn) -> bool {
        if self.focused_column == column {
            return false;
        }

        self.focused_column = column;
        true
    }

    pub(crate) fn anchor_bounds(&self) -> Option<Bounds<Pixels>> {
        self.anchor_bounds
    }

    pub(crate) fn rename_editor_open(&self) -> bool {
        self.rename_editor_target.is_some()
    }

    pub(crate) fn rename_editor_open_for(&self, workspace_id: &BerylWorkspaceId) -> bool {
        self.rename_editor_target
            .as_ref()
            .is_some_and(|target| target == workspace_id)
    }

    pub(crate) fn rename_editor_target(&self) -> Option<&BerylWorkspaceId> {
        self.rename_editor_target.as_ref()
    }

    pub(crate) fn open(&mut self) {
        self.open = true;
        self.focused_column = WorkspacePickerFocusedColumn::Workspaces;
        self.rename_editor_target = None;
        self.row_action_menu = None;
        self.delete_hold = None;
        self.runtime_selector_dropdown.close();
        self.member_action_menu = None;
        self.popup_bounds = None;
    }

    pub(crate) fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.rename_editor_target = None;
        self.row_action_menu = None;
        self.delete_hold = None;
        self.runtime_selector_dropdown.close();
        self.member_action_menu = None;
        self.popup_bounds = None;
    }

    pub(crate) fn open_rename_editor_for(&mut self, workspace_id: BerylWorkspaceId) {
        self.rename_editor_target = Some(workspace_id);
        self.row_action_menu = None;
        self.delete_hold = None;
        self.runtime_selector_dropdown.close();
        self.member_action_menu = None;
    }

    pub(crate) fn close_rename_editor(&mut self) {
        self.rename_editor_target = None;
    }

    pub(crate) fn row_action_menu_active(&self) -> Option<&WorkspaceRowActionMenuOpen> {
        self.row_action_menu.as_ref()
    }

    pub(crate) fn row_action_menu_is_open(&self) -> bool {
        self.row_action_menu.is_some()
    }

    pub(crate) fn open_row_action_menu(
        &mut self,
        workspace_id: BerylWorkspaceId,
        position: Point<Pixels>,
    ) {
        self.rename_editor_target = None;
        self.delete_hold = None;
        self.runtime_selector_dropdown.close();
        self.member_action_menu = None;
        self.row_action_menu = Some(WorkspaceRowActionMenuOpen {
            workspace_id,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close_row_action_menu(&mut self) -> bool {
        let menu_changed = self.row_action_menu.take().is_some();
        let hold_changed = self.delete_hold.take().is_some();
        menu_changed || hold_changed
    }

    pub(crate) fn set_row_action_menu_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(menu) = self.row_action_menu.as_mut() {
            menu.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_row_action_menu_for_mouse_down(
        &self,
        position: Point<Pixels>,
    ) -> bool {
        self.row_action_menu
            .as_ref()
            .is_some_and(|menu| !menu.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

    pub(crate) fn runtime_selector_dropdown(&self) -> &RuntimeSelectorDropdownState {
        &self.runtime_selector_dropdown
    }

    pub(crate) fn runtime_selector_dropdown_is_open(&self) -> bool {
        self.runtime_selector_dropdown.is_open()
    }

    pub(crate) fn toggle_runtime_selector_dropdown(&mut self, item_count: usize) -> bool {
        self.rename_editor_target = None;
        self.row_action_menu = None;
        self.delete_hold = None;
        self.member_action_menu = None;
        self.runtime_selector_dropdown.toggle(item_count)
    }

    pub(crate) fn close_runtime_selector_dropdown(&mut self) -> bool {
        self.runtime_selector_dropdown.close()
    }

    pub(crate) fn set_runtime_selector_trigger_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.runtime_selector_dropdown.set_trigger_bounds(bounds);
    }

    pub(crate) fn set_runtime_selector_dropdown_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.runtime_selector_dropdown.set_dropdown_bounds(bounds);
    }

    pub(crate) fn move_runtime_selector_highlight(
        &mut self,
        delta: isize,
        item_count: usize,
    ) -> bool {
        self.runtime_selector_dropdown
            .move_highlight(delta, item_count)
    }

    pub(crate) fn should_dismiss_runtime_selector_dropdown_for_mouse_down(
        &self,
        position: Point<Pixels>,
    ) -> bool {
        self.runtime_selector_dropdown
            .should_dismiss_for_mouse_down(position)
    }

    pub(crate) fn member_action_menu_active(&self) -> Option<&WorkspaceMemberActionMenuOpen> {
        self.member_action_menu.as_ref()
    }

    pub(crate) fn member_action_menu_is_open(&self) -> bool {
        self.member_action_menu.is_some()
    }

    pub(crate) fn open_member_action_menu(
        &mut self,
        member_id: WorkspaceMemberId,
        position: Point<Pixels>,
    ) {
        self.rename_editor_target = None;
        self.row_action_menu = None;
        self.delete_hold = None;
        self.runtime_selector_dropdown.close();
        self.member_action_menu = Some(WorkspaceMemberActionMenuOpen {
            member_id,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close_member_action_menu(&mut self) -> bool {
        self.member_action_menu.take().is_some()
    }

    pub(crate) fn set_member_action_menu_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(menu) = self.member_action_menu.as_mut() {
            menu.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_member_action_menu_for_mouse_down(
        &self,
        position: Point<Pixels>,
    ) -> bool {
        self.member_action_menu
            .as_ref()
            .is_some_and(|menu| !menu.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

    pub(crate) fn delete_hold_active(&self) -> bool {
        self.delete_hold.is_some()
    }

    pub(crate) fn delete_hold_progress_for_target(
        &self,
        workspace_id: &BerylWorkspaceId,
        now: Instant,
    ) -> Option<f32> {
        let hold = self.delete_hold.as_ref()?;
        (hold.workspace_id == *workspace_id).then(|| hold.progress(now))
    }

    pub(crate) fn begin_delete_hold(
        &mut self,
        workspace_id: BerylWorkspaceId,
        source: WorkspaceDeleteHoldSource,
        now: Instant,
    ) -> bool {
        let target_is_active_menu = self
            .row_action_menu
            .as_ref()
            .is_some_and(|menu| menu.workspace_id == workspace_id);
        if !target_is_active_menu {
            return false;
        }

        if self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.workspace_id == workspace_id && hold.source == source)
        {
            return false;
        }

        self.delete_hold = Some(WorkspaceDeleteHoldState {
            workspace_id,
            source,
            started_at: now,
            duration: WORKSPACE_DELETE_HOLD_DURATION,
        });
        true
    }

    pub(crate) fn cancel_delete_hold(&mut self) -> bool {
        self.delete_hold.take().is_some()
    }

    pub(crate) fn cancel_delete_hold_source(&mut self, source: WorkspaceDeleteHoldSource) -> bool {
        let matches_source = self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.source == source);
        if matches_source {
            self.delete_hold = None;
        }
        matches_source
    }

    pub(crate) fn cancel_delete_hold_for_stale_target(&mut self, target_exists: bool) -> bool {
        let should_cancel = self.delete_hold.is_some() && !target_exists;
        if should_cancel {
            self.delete_hold = None;
        }
        should_cancel
    }

    pub(crate) fn complete_delete_hold_if_ready(
        &mut self,
        now: Instant,
    ) -> Option<BerylWorkspaceId> {
        let ready = self
            .delete_hold
            .as_ref()
            .is_some_and(|hold| hold.is_complete(now));
        if ready {
            return self.delete_hold.take().map(|hold| hold.workspace_id);
        }
        None
    }

    pub(crate) fn set_anchor_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.anchor_bounds = bounds;
    }

    pub(crate) fn set_popup_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.popup_bounds = bounds;
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            && !self
                .popup_bounds
                .is_some_and(|bounds| bounds.contains(&position))
            && !self
                .anchor_bounds
                .is_some_and(|bounds| bounds.contains(&position))
            && !self
                .row_action_menu
                .as_ref()
                .is_some_and(|menu| menu.bounds.is_some_and(|bounds| bounds.contains(&position)))
            && !self.runtime_selector_dropdown.contains_bounds(position)
            && !self
                .member_action_menu
                .as_ref()
                .is_some_and(|menu| menu.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }
}

impl WorkspaceRowActionMenuOpen {
    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }
}

impl RuntimeSelectorDropdownState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn highlighted_index(&self) -> usize {
        self.highlighted_index
    }

    fn toggle(&mut self, item_count: usize) -> bool {
        if self.open {
            return self.close();
        }

        self.open = true;
        self.highlighted_index = self.highlighted_index.min(item_count.saturating_sub(1));
        self.dropdown_bounds = None;
        true
    }

    fn close(&mut self) -> bool {
        let was_open = self.open;
        self.open = false;
        self.dropdown_bounds = None;
        was_open
    }

    fn set_trigger_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.trigger_bounds = bounds;
    }

    fn set_dropdown_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        self.dropdown_bounds = bounds;
    }

    fn move_highlight(&mut self, delta: isize, item_count: usize) -> bool {
        if item_count == 0 {
            return false;
        }

        let current = self.highlighted_index.min(item_count - 1) as isize;
        let next = (current + delta).clamp(0, item_count.saturating_sub(1) as isize) as usize;
        if next == self.highlighted_index {
            return false;
        }

        self.highlighted_index = next;
        true
    }

    fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open && !self.contains_bounds(position)
    }

    fn contains_bounds(&self, position: Point<Pixels>) -> bool {
        self.trigger_bounds
            .is_some_and(|bounds| bounds.contains(&position))
            || self
                .dropdown_bounds
                .is_some_and(|bounds| bounds.contains(&position))
    }
}

impl WorkspaceMemberActionMenuOpen {
    pub(crate) fn member_id(&self) -> &WorkspaceMemberId {
        &self.member_id
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }
}

impl RuntimeSelectorDistroList {
    pub(crate) fn status(&self) -> &RuntimeSelectorDistroListStatus {
        &self.status
    }

    pub(crate) fn distro_names(&self) -> &[String] {
        &self.distro_names
    }

    pub(crate) fn begin_loading(&mut self) -> bool {
        if matches!(self.status, RuntimeSelectorDistroListStatus::Loading) {
            return false;
        }

        self.status = RuntimeSelectorDistroListStatus::Loading;
        true
    }

    pub(crate) fn finish_loading(&mut self, result: Result<Vec<String>, String>) {
        match result {
            Ok(mut distro_names) => {
                distro_names.sort();
                distro_names.dedup();
                self.distro_names = distro_names;
                self.status = RuntimeSelectorDistroListStatus::Loaded;
            }
            Err(error) => {
                self.distro_names.clear();
                self.status = RuntimeSelectorDistroListStatus::Failed(error);
            }
        }
    }

    pub(crate) fn should_refresh(&self) -> bool {
        matches!(
            self.status,
            RuntimeSelectorDistroListStatus::NotLoaded | RuntimeSelectorDistroListStatus::Failed(_)
        )
    }
}

impl WorkspaceDeleteHoldState {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn is_complete(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }
}

pub(crate) const CREATE_NEW_ITEM_INDEX: usize = 0;

pub(crate) fn workspace_picker_item_count(workspace_count: usize) -> usize {
    workspace_count.saturating_add(1)
}

pub(crate) fn workspace_item_index(workspace_index: usize) -> usize {
    workspace_index.saturating_add(1)
}

pub(crate) fn workspace_index_for_item_index(item_index: usize) -> Option<usize> {
    item_index.checked_sub(1)
}

pub(crate) fn filtered_workspace_indices(
    workspaces: &[BerylWorkspaceManifest],
    member_paths: &WorkspacePickerMemberPaths,
    filter: &str,
) -> Vec<usize> {
    let query = filter.trim().to_lowercase();
    workspaces
        .iter()
        .enumerate()
        .filter_map(|(index, workspace)| {
            workspace_matches_filter(
                workspace,
                member_paths
                    .get(workspace.id())
                    .map_or(&[][..], Vec::as_slice),
                &query,
            )
            .then_some(index)
        })
        .collect()
}

pub(crate) fn workspace_index_for_filtered_item_index(
    item_index: usize,
    visible_workspace_indices: &[usize],
) -> Option<usize> {
    let visible_workspace_index = workspace_index_for_item_index(item_index)?;
    visible_workspace_indices
        .get(visible_workspace_index)
        .copied()
}

fn workspace_matches_filter(
    workspace: &BerylWorkspaceManifest,
    explicit_member_paths: &[String],
    query: &str,
) -> bool {
    query.is_empty()
        || workspace.title().to_lowercase().contains(query)
        || explicit_member_paths
            .iter()
            .any(|path| path.to_lowercase().contains(query))
}

pub(crate) fn workspace_row_accepts_activation(rename_editor_open_for_row: bool) -> bool {
    !rename_editor_open_for_row
}

pub(crate) fn workspace_picker_transition_disabled_reason(
    blockers: WorkspacePickerTransitionBlockers,
) -> Option<&'static str> {
    (blockers.edit_rollback_work || blockers.edit_replacement_work)
        .then_some(WORKSPACE_PICKER_THREAD_EDIT_WAIT_NOTICE)
}

pub(crate) fn workspace_picker_transition_path_disabled_reason(
    path: WorkspacePickerTransitionPath,
    blockers: WorkspacePickerTransitionBlockers,
) -> Option<&'static str> {
    match path {
        WorkspacePickerTransitionPath::SwitchWorkspace
        | WorkspacePickerTransitionPath::CreateWorkspace
        | WorkspacePickerTransitionPath::DeleteWorkspace => {
            workspace_picker_transition_disabled_reason(blockers)
        }
    }
}

pub(crate) fn explicit_member_path_strings(
    workspace_state: &WorkspaceConversationState,
) -> Vec<String> {
    workspace_state
        .explicit_members()
        .iter()
        .map(|member| member.canonical_path().display().to_string())
        .collect()
}

pub(crate) fn workspace_picker_member_paths_from_states(
    known_workspaces: &[BerylWorkspaceManifest],
    mut load_workspace_state: impl FnMut(&BerylWorkspaceId) -> Option<WorkspaceConversationState>,
) -> WorkspacePickerMemberPaths {
    known_workspaces
        .iter()
        .map(|workspace| {
            let explicit_member_paths = load_workspace_state(workspace.id())
                .map(|workspace_state| explicit_member_path_strings(&workspace_state))
                .unwrap_or_default();
            (workspace.id().clone(), explicit_member_paths)
        })
        .collect()
}

pub(crate) fn popup_width(viewport_width: Pixels) -> Pixels {
    let margin_cap = viewport_width - px(layout::WORKSPACE_PICKER_MARGIN * 2.0);
    let viewport_cap = viewport_width * layout::WORKSPACE_PICKER_VIEWPORT_WIDTH_RATIO;
    let available = margin_cap.min(viewport_cap).max(px(0.0));
    if available < px(layout::WORKSPACE_PICKER_MIN_WIDTH) {
        return available;
    }

    px(layout::WORKSPACE_PICKER_PREFERRED_WIDTH)
        .max(px(layout::WORKSPACE_PICKER_MIN_WIDTH))
        .min(available)
}

pub(crate) fn popup_layout(
    visible_workspace_count: usize,
    member_list_item_count: usize,
    runtime_selector_dropdown_row_count: usize,
    viewport_width: Pixels,
    viewport_height: Pixels,
) -> WorkspacePickerPopupLayout {
    let width = popup_width(viewport_width);
    let divider_width = px(layout::WORKSPACE_PICKER_COLUMN_DIVIDER_WIDTH).min(width);
    let column_width = (width - divider_width).max(px(0.0));
    let preferred_columns = layout::WORKSPACE_PICKER_WORKSPACES_COLUMN_PREFERRED_WIDTH
        + layout::WORKSPACE_PICKER_MEMBERS_COLUMN_PREFERRED_WIDTH;
    let workspaces_column_ratio = if preferred_columns <= 0.0 {
        0.5
    } else {
        layout::WORKSPACE_PICKER_WORKSPACES_COLUMN_PREFERRED_WIDTH / preferred_columns
    };
    let workspaces_column_width = column_width * workspaces_column_ratio;
    let members_column_width = column_width - workspaces_column_width;
    let workspaces_list_content_height = workspaces_list_content_height(visible_workspace_count);
    let members_list_content_height = members_list_content_height(member_list_item_count);
    let runtime_selector_dropdown_content_height =
        runtime_selector_dropdown_content_height(runtime_selector_dropdown_row_count);
    let members_column_content_height =
        px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_HEIGHT) + members_list_content_height;
    let members_dropdown_content_height = if runtime_selector_dropdown_row_count == 0 {
        px(0.0)
    } else {
        px(layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP)
            + runtime_selector_dropdown_content_height
    };
    let column_content_height = (px(layout::WORKSPACE_PICKER_FILTER_HEIGHT)
        + workspaces_list_content_height)
        .max(members_column_content_height.max(members_dropdown_content_height));
    let desired = px(layout::WORKSPACE_PICKER_HEADER_HEIGHT) + column_content_height;
    let max_height = (viewport_height * layout::WORKSPACE_PICKER_MAX_HEIGHT_RATIO).max(px(0.0));
    let height = desired.min(max_height);
    let column_available_height =
        (height - px(layout::WORKSPACE_PICKER_HEADER_HEIGHT)).max(px(0.0));
    let runtime_selector_dropdown_height = if runtime_selector_dropdown_row_count == 0 {
        px(0.0)
    } else {
        (column_available_height
            - px(layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP))
        .max(px(0.0))
        .min(runtime_selector_dropdown_content_height)
    };

    WorkspacePickerPopupLayout {
        width,
        height,
        workspaces_column_width,
        members_column_width,
        divider_width,
        workspaces_list_height: (column_available_height
            - px(layout::WORKSPACE_PICKER_FILTER_HEIGHT))
        .max(px(0.0)),
        members_list_height: (column_available_height
            - px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_HEIGHT))
        .max(px(0.0)),
        runtime_selector_dropdown_height,
    }
}

pub(crate) fn workspace_picker_member_list_item_count(
    explicit_member_count: usize,
    runtime_selected: bool,
) -> usize {
    1 + if runtime_selected {
        explicit_member_count.max(1)
    } else {
        0
    }
}

pub(crate) fn runtime_selector_item_count(distro_names: &[String]) -> usize {
    1 + distro_names.len()
}

pub(crate) fn runtime_selector_dropdown_row_count(
    distro_list: &RuntimeSelectorDistroList,
) -> usize {
    let wsl_row_count = match distro_list.status() {
        RuntimeSelectorDistroListStatus::Loaded | RuntimeSelectorDistroListStatus::NotLoaded
            if !distro_list.distro_names().is_empty() =>
        {
            distro_list.distro_names().len()
        }
        RuntimeSelectorDistroListStatus::Loaded
        | RuntimeSelectorDistroListStatus::NotLoaded
        | RuntimeSelectorDistroListStatus::Loading
        | RuntimeSelectorDistroListStatus::Failed(_) => 1,
    };
    1 + wsl_row_count
}

pub(crate) fn runtime_selector_row_for_index(
    distro_names: &[String],
    index: usize,
) -> Option<RuntimeSelectorRow> {
    if index == 0 {
        return Some(RuntimeSelectorRow::HostWindows);
    }

    distro_names
        .get(index.saturating_sub(1))
        .map(|distro_name| RuntimeSelectorRow::WslDistro {
            distro_name: distro_name.clone(),
        })
}

pub(crate) fn runtime_selector_row_label(row: &RuntimeSelectorRow) -> String {
    match row {
        RuntimeSelectorRow::HostWindows => "host-Windows".to_string(),
        RuntimeSelectorRow::WslDistro { distro_name } => format!("WSL: {distro_name}"),
    }
}

pub(crate) fn runtime_selector_row_runtime(row: &RuntimeSelectorRow) -> RuntimeMode {
    match row {
        RuntimeSelectorRow::HostWindows => RuntimeMode::HostWindows,
        RuntimeSelectorRow::WslDistro { distro_name } => RuntimeMode::WslLinux {
            distro_name: distro_name.clone(),
        },
    }
}

fn workspaces_list_content_height(visible_workspace_count: usize) -> Pixels {
    px(layout::WORKSPACE_PICKER_CREATE_ROW_HEIGHT)
        + px(layout::WORKSPACE_PICKER_ROW_HEIGHT) * visible_workspace_count as f32
}

fn members_list_content_height(member_list_item_count: usize) -> Pixels {
    if member_list_item_count == 0 {
        return px(0.0);
    }

    px(layout::WORKSPACE_PICKER_MEMBERS_ATTACH_ROW_HEIGHT)
        + px(layout::WORKSPACE_PICKER_MEMBERS_ROW_HEIGHT)
            * member_list_item_count.saturating_sub(1) as f32
}

fn runtime_selector_dropdown_content_height(row_count: usize) -> Pixels {
    let visible_rows = row_count.min(layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_MAX_VISIBLE_ROWS);
    px(layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_ROW_HEIGHT) * visible_rows as f32
}
