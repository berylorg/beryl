use beryl_home_store::CommandCancellation;

use super::*;

pub(in super::super) enum PropagatedClipboardAction {
    Request(RangeTextInputRequest),
    Write(ClipboardWriteRequest),
    ContiguousLimitExceeded,
    Cancelled,
}

pub(in super::super) struct ActivePropagatedClipboard {
    coordinator: RangeClipboardCoordinator,
    progress: ClipboardProgress,
    next_request_id: u64,
    cancellation: CommandCancellation,
}

impl ActivePropagatedClipboard {
    pub(in super::super) fn new(
        selection: MainWindowComposerSelectionIdentity,
        selected_range: RangeSourceSelection,
        kind: ClipboardKind,
        limits: ClipboardLimits,
    ) -> Result<Self, String> {
        if limits.provenance() != gpui_text_input::ClipboardProvenancePolicy::Omit {
            return Err("composer clipboard provenance is not enabled in this phase".to_owned());
        }
        let mut coordinator = RangeClipboardCoordinator::new_composite(
            selection.binding().range_binding(),
            gpui_text_input::PresentationGeneration::new(
                selection.binding().presentation_generation().get(),
            ),
            TextInputAtomClipboardPolicy::PlainText,
            limits,
        )
        .map_err(|_| "composer clipboard coordinator construction failed".to_owned())?;
        let progress = coordinator
            .begin_selection(
                ClipboardId::new(1),
                kind,
                selected_range.anchor,
                selected_range.head,
            )
            .map_err(|_| "composer clipboard selection was rejected".to_owned())?;
        Ok(Self {
            coordinator,
            progress,
            next_request_id: 1,
            cancellation: CommandCancellation::new(),
        })
    }

    pub(in super::super) fn cancellation(&self) -> CommandCancellation {
        self.cancellation.clone()
    }

    pub(in super::super) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(in super::super) fn next_action(&mut self) -> Result<PropagatedClipboardAction, String> {
        match &self.progress {
            ClipboardProgress::NeedTextPage { key, .. } => {
                let request = self
                    .coordinator
                    .request_text_page(*key, PageRequestId::new(self.next_request_id))
                    .map_err(|_| "composer clipboard text request was rejected".to_owned())?;
                self.next_request_id = self
                    .next_request_id
                    .checked_add(1)
                    .ok_or_else(|| "composer clipboard request identity exhausted".to_owned())?;
                Ok(PropagatedClipboardAction::Request(
                    RangeTextInputRequest::Page(request),
                ))
            }
            ClipboardProgress::NeedObjectPage { key, .. } => {
                let request = self
                    .coordinator
                    .request_object_page(*key, ObjectRequestId::new(self.next_request_id))
                    .map_err(|_| "composer clipboard object request was rejected".to_owned())?;
                self.next_request_id = self
                    .next_request_id
                    .checked_add(1)
                    .ok_or_else(|| "composer clipboard request identity exhausted".to_owned())?;
                Ok(PropagatedClipboardAction::Request(
                    RangeTextInputRequest::ObjectPage(request),
                ))
            }
            ClipboardProgress::ProvenancePage(page) => {
                let _ = self.coordinator.cancel(page.key().clipboard());
                self.progress = ClipboardProgress::Terminal(ClipboardCompletion::Cancelled);
                Err("composer clipboard provenance arrived before rich paste is enabled".to_owned())
            }
            ClipboardProgress::Write(_) => {
                let ClipboardProgress::Write(write) = std::mem::replace(
                    &mut self.progress,
                    ClipboardProgress::Terminal(ClipboardCompletion::Cancelled),
                ) else {
                    unreachable!("clipboard progress was observed as a write")
                };
                Ok(PropagatedClipboardAction::Write(write))
            }
            ClipboardProgress::Terminal(
                ClipboardCompletion::TooLarge | ClipboardCompletion::TextPageTooLarge,
            ) => Ok(PropagatedClipboardAction::ContiguousLimitExceeded),
            ClipboardProgress::Terminal(ClipboardCompletion::Cancelled) => {
                Ok(PropagatedClipboardAction::Cancelled)
            }
            ClipboardProgress::Terminal(_) => {
                Err("composer clipboard terminated unexpectedly".into())
            }
        }
    }

    pub(in super::super) fn admit(
        &mut self,
        outcome: MainWindowComposerDispatchOutcome,
    ) -> Result<(), String> {
        self.progress = match (&self.progress, outcome) {
            (
                ClipboardProgress::NeedTextPage { .. },
                MainWindowComposerDispatchOutcome::Page(page),
            ) => self.admit_text_page(page)?,
            (
                ClipboardProgress::NeedObjectPage { .. },
                MainWindowComposerDispatchOutcome::ObjectPage(page),
            ) => self.admit_object_page(page)?,
            _ => return Err("composer clipboard response had the wrong kind".into()),
        };
        Ok(())
    }

    fn admit_text_page(&mut self, page: RangePage) -> Result<ClipboardProgress, String> {
        let prepared = self
            .coordinator
            .prepare_text_page(&page)
            .map_err(|_| "composer clipboard text response preparation failed".to_owned())?;
        let commit = self
            .coordinator
            .commit_text_page(page, prepared)
            .map_err(|_| "composer clipboard text response commit failed".to_owned())?;
        self.finish_prepared(commit)
    }

    fn admit_object_page(&mut self, page: ObjectPage) -> Result<ClipboardProgress, String> {
        let prepared = self
            .coordinator
            .prepare_object_page(&page)
            .map_err(|_| "composer clipboard object response preparation failed".to_owned())?;
        let commit = self
            .coordinator
            .commit_object_page(page, prepared)
            .map_err(|_| "composer clipboard object response commit failed".to_owned())?;
        self.finish_prepared(commit)
    }

    fn finish_prepared(
        &mut self,
        mut commit: ClipboardPreparedCommit,
    ) -> Result<ClipboardProgress, String> {
        loop {
            if let Some(progress) = commit.into_progress() {
                return Ok(progress);
            }
            let prepared = self
                .coordinator
                .prepare_next()
                .map_err(|_| "composer clipboard response continuation failed".to_owned())?;
            commit = self
                .coordinator
                .commit_prepared(prepared)
                .map_err(|_| "composer clipboard response continuation commit failed".to_owned())?;
        }
    }

    pub(in super::super) fn acknowledge_write(
        &mut self,
        key: gpui_text_input::ClipboardKey,
        outcome: gpui_text_input::ClipboardWriteOutcome,
    ) -> Result<ClipboardCompletion, String> {
        let completion = self
            .coordinator
            .acknowledge_write(key, outcome)
            .map_err(|_| "composer clipboard write acknowledgement was rejected".to_owned())?;
        self.progress = ClipboardProgress::Terminal(completion.clone());
        Ok(completion)
    }
}
