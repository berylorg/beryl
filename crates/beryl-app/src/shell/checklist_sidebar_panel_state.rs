use beryl_model::semantic_graph::SemanticNodeId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChecklistSidebarPanelState {
    checklist_id: Option<SemanticNodeId>,
    row_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChecklistSidebarProjectionSync {
    Unchanged,
    ResetScroll,
    ClampScroll,
}

impl ChecklistSidebarPanelState {
    pub(crate) fn sync_projection(
        &mut self,
        checklist_id: Option<&SemanticNodeId>,
        row_count: usize,
    ) -> ChecklistSidebarProjectionSync {
        let next_checklist_id = checklist_id.cloned();
        let previous_checklist_id = self.checklist_id.clone();
        let previous_row_count = self.row_count;

        self.checklist_id = next_checklist_id.clone();
        self.row_count = row_count;

        if previous_checklist_id != next_checklist_id {
            ChecklistSidebarProjectionSync::ResetScroll
        } else if previous_row_count != row_count {
            ChecklistSidebarProjectionSync::ClampScroll
        } else {
            ChecklistSidebarProjectionSync::Unchanged
        }
    }
}
