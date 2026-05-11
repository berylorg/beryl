use beryl_model::semantic_graph::{
    ChecklistItemStatus, SemanticGraph, SemanticNode, SemanticNodeId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChecklistSidebarRow {
    pub(crate) node_id: SemanticNodeId,
    pub(crate) number: usize,
    pub(crate) title: String,
    pub(crate) status: Option<ChecklistItemStatus>,
    pub(crate) status_label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChecklistSidebarProjection {
    checklist_id: SemanticNodeId,
    title: String,
    rows: Vec<ChecklistSidebarRow>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChecklistSidebarProjectionRefresh {
    changed: bool,
    selected_checklist_changed: bool,
    previous_row_count: usize,
    row_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChecklistSidebarProjectionCache {
    projection: Option<ChecklistSidebarProjection>,
}

impl ChecklistSidebarRow {
    pub(crate) fn element_key(&self) -> String {
        format!("checklist-item-row-{}", self.node_id.as_str())
    }
}

impl ChecklistSidebarProjection {
    pub(crate) fn checklist_id(&self) -> &SemanticNodeId {
        &self.checklist_id
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn row(&self, index: usize) -> Option<&ChecklistSidebarRow> {
        self.rows.get(index)
    }
}

impl ChecklistSidebarProjectionRefresh {
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }

    #[cfg(test)]
    pub(crate) fn selected_checklist_changed(&self) -> bool {
        self.selected_checklist_changed
    }

    #[cfg(test)]
    pub(crate) fn previous_row_count(&self) -> usize {
        self.previous_row_count
    }

    #[cfg(test)]
    pub(crate) fn row_count(&self) -> usize {
        self.row_count
    }
}

impl ChecklistSidebarProjectionCache {
    pub(crate) fn projection(&self) -> Option<&ChecklistSidebarProjection> {
        self.projection.as_ref()
    }

    pub(crate) fn refresh(
        &mut self,
        graph: &SemanticGraph,
        selected_checklist_id: Option<&SemanticNodeId>,
    ) -> ChecklistSidebarProjectionRefresh {
        let previous_checklist_id = self
            .projection
            .as_ref()
            .map(|projection| projection.checklist_id.clone());
        let previous_row_count = self
            .projection
            .as_ref()
            .map_or(0, ChecklistSidebarProjection::row_count);
        let next_projection = selected_checklist_id.and_then(|node_id| {
            let checklist = graph.node(node_id)?;
            checklist
                .facets()
                .has_checklist()
                .then(|| project_checklist_projection(graph, checklist))
        });
        let next_checklist_id = next_projection
            .as_ref()
            .map(|projection| projection.checklist_id.clone());
        let row_count = next_projection
            .as_ref()
            .map_or(0, ChecklistSidebarProjection::row_count);
        let changed = self.projection != next_projection;
        let selected_checklist_changed = previous_checklist_id != next_checklist_id;

        if changed {
            self.projection = next_projection;
        }

        ChecklistSidebarProjectionRefresh {
            changed,
            selected_checklist_changed,
            previous_row_count,
            row_count,
        }
    }
}

pub(crate) fn project_checklist_projection(
    graph: &SemanticGraph,
    checklist: &SemanticNode,
) -> ChecklistSidebarProjection {
    let rows = graph
        .checklist_items(checklist.id())
        .into_iter()
        .enumerate()
        .map(|(index, item)| ChecklistSidebarRow {
            node_id: item.id().clone(),
            number: index + 1,
            title: item.title().to_string(),
            status: item.checklist_item_status(),
            status_label: checklist_status_label(item.checklist_item_status()),
        })
        .collect();

    ChecklistSidebarProjection {
        checklist_id: checklist.id().clone(),
        title: checklist.title().to_string(),
        rows,
    }
}

pub(crate) fn checklist_status_label(status: Option<ChecklistItemStatus>) -> &'static str {
    match status.unwrap_or(ChecklistItemStatus::Todo) {
        ChecklistItemStatus::Todo => "todo",
        ChecklistItemStatus::InProgress => "doing",
        ChecklistItemStatus::Done => "done",
    }
}
