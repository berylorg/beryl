use beryl_model::semantic_graph::SemanticNodeId;

#[derive(Clone, Debug, Default)]
pub(crate) struct ChecklistSidebarVisibilityState {
    visible: bool,
    manually_shown: bool,
    hidden_for_checklist: Option<SemanticNodeId>,
}

impl ChecklistSidebarVisibilityState {
    pub(crate) fn visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn toggle(&mut self, selected_checklist: Option<&SemanticNodeId>) -> bool {
        let before = self.visible;
        if self.visible {
            self.visible = false;
            self.manually_shown = false;
            self.hidden_for_checklist = selected_checklist.cloned();
        } else {
            self.visible = true;
            self.manually_shown = true;
            self.hidden_for_checklist = None;
        }
        before != self.visible
    }

    pub(crate) fn reconcile_selection(
        &mut self,
        selected_checklist: Option<&SemanticNodeId>,
    ) -> bool {
        let before = (
            self.visible,
            self.manually_shown,
            self.hidden_for_checklist.clone(),
        );
        match selected_checklist {
            Some(selected_checklist) => {
                if self.hidden_for_checklist.as_ref() == Some(selected_checklist) {
                    return false;
                }
                self.visible = true;
                self.hidden_for_checklist = None;
            }
            None => {
                self.hidden_for_checklist = None;
                if !self.manually_shown {
                    self.visible = false;
                }
            }
        }
        before
            != (
                self.visible,
                self.manually_shown,
                self.hidden_for_checklist.clone(),
            )
    }
}
