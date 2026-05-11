pub(crate) const WORKSPACE_RENAME_WAIT_TOOLTIP: &str =
    "Wait for in-progress workspace work to finish before renaming.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceRenameBlockers {
    pub(crate) workspace_lifecycle: bool,
    pub(crate) graph_work: bool,
    pub(crate) transcript_work: bool,
    pub(crate) inventory_work: bool,
    pub(crate) image_work: bool,
    pub(crate) status_work: bool,
    pub(crate) title_work: bool,
    pub(crate) member_work: bool,
    pub(crate) picker_work: bool,
    pub(crate) persistence_work: bool,
}

impl WorkspaceRenameBlockers {
    pub(crate) fn any(self) -> bool {
        self.workspace_lifecycle
            || self.graph_work
            || self.transcript_work
            || self.inventory_work
            || self.image_work
            || self.status_work
            || self.title_work
            || self.member_work
            || self.picker_work
            || self.persistence_work
    }
}

pub(crate) fn workspace_rename_disabled_reason(
    blockers: WorkspaceRenameBlockers,
) -> Option<&'static str> {
    blockers.any().then_some(WORKSPACE_RENAME_WAIT_TOOLTIP)
}
