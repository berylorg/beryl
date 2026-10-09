use super::*;

impl MainWindowShellRoot {
    pub(crate) fn test_original_claim_page_release_evidence(
        &self,
    ) -> Vec<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        u64,
        u64,
        usize,
    )> {
        self.running_threads
            .fixture_claim_page_release_evidence
            .clone()
    }

    pub(crate) fn test_original_claim_widget_release(
        &self,
    ) -> Option<(
        crate::main_window::MainWindowClaimRetirementKind,
        crate::main_window::MainWindowComposerWidgetRelease,
    )> {
        self.running_threads.fixture_claim_widget_release
    }
}
