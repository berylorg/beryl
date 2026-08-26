use super::*;

pub(in super::super) enum PropagatedClipboardCollection {
    Ready {
        coordinator: RangeClipboardCoordinator,
        write: ClipboardWriteRequest,
    },
    Rejected,
}

pub(in super::super) fn collect(
    service: &Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    selected_range: RangeSourceSelection,
    kind: ClipboardKind,
    limits: ClipboardLimits,
) -> Result<PropagatedClipboardCollection, String> {
    let mut coordinator = RangeClipboardCoordinator::new_composite(
        selection.binding().range_binding(),
        gpui_text_input::PresentationGeneration::new(
            selection.binding().presentation_generation().get(),
        ),
        TextInputAtomClipboardPolicy::PlainText,
        limits,
    );
    let mut progress = coordinator
        .begin_selection(
            ClipboardId::new(1),
            kind,
            selected_range.anchor,
            selected_range.head,
        )
        .map_err(|error| error.to_string())?;
    let mut request_id = 1_u64;
    let mut remaining_steps = limits.max_bytes().saturating_add(8);
    let cancellation = CommandCancellation::new();
    let mut slot = service
        .slot
        .lock()
        .map_err(|_| "conversation composer service lock failed".to_owned())?;

    while remaining_steps > 0 {
        remaining_steps -= 1;
        progress = match progress {
            ClipboardProgress::NeedTextPage { key, .. } => {
                let request = coordinator
                    .request_text_page(key, PageRequestId::new(request_id))
                    .map_err(|error| error.to_string())?;
                request_id = request_id
                    .checked_add(1)
                    .ok_or_else(|| "composer clipboard request identity exhausted".to_owned())?;
                let request_diagnostic = format!("{request:?}");
                let outcome = slot
                    .dispatch_selected_request(
                        &service.store,
                        selection,
                        RangeTextInputRequest::Page(request),
                        Box::new([]),
                        &cancellation,
                    )
                    .map_err(|error| {
                        format!(
                            "composer clipboard text request {request_diagnostic} failed: {error}"
                        )
                    })?;
                let MainWindowComposerDispatchOutcome::Page(page) = outcome else {
                    return Err(
                        "composer clipboard text request returned the wrong response".into(),
                    );
                };
                coordinator
                    .admit_text_page(page)
                    .map_err(|error| error.to_string())?
            }
            ClipboardProgress::NeedObjectPage { key, .. } => {
                let request = coordinator
                    .request_object_page(key, ObjectRequestId::new(request_id))
                    .map_err(|error| error.to_string())?;
                request_id = request_id
                    .checked_add(1)
                    .ok_or_else(|| "composer clipboard request identity exhausted".to_owned())?;
                let request_diagnostic = format!("{request:?}");
                let outcome = slot
                    .dispatch_selected_request(
                        &service.store,
                        selection,
                        RangeTextInputRequest::ObjectPage(request),
                        Box::new([]),
                        &cancellation,
                    )
                    .map_err(|error| {
                        format!(
                            "composer clipboard marker request {request_diagnostic} failed: {error}"
                        )
                    })?;
                let MainWindowComposerDispatchOutcome::ObjectPage(page) = outcome else {
                    return Err(
                        "composer clipboard marker request returned the wrong response".into(),
                    );
                };
                coordinator
                    .admit_object_page(page)
                    .map_err(|error| error.to_string())?
            }
            ClipboardProgress::Write(write) => {
                return Ok(PropagatedClipboardCollection::Ready { coordinator, write });
            }
            ClipboardProgress::Terminal(
                ClipboardCompletion::TooLarge | ClipboardCompletion::TextPageTooLarge,
            ) => return Ok(PropagatedClipboardCollection::Rejected),
            ClipboardProgress::Terminal(completion) => {
                return Err(format!(
                    "composer clipboard collection terminated unexpectedly: {completion:?}"
                ));
            }
        };
    }

    Err("composer clipboard collection exceeded its bounded request budget".to_owned())
}
