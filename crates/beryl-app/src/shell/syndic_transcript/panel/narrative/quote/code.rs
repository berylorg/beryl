use super::*;

#[derive(Clone)]
pub(super) struct CodeDiscussionCapture {
    pub(super) id: u64,
    pub(super) owner: crate::syndic_transcript::ResourceTranscriptDemand,
    pub(super) activation: u64,
    pub(super) presentation: u64,
    root: [u8; 32],
    binding: crate::syndic_transcript::provider::resource_index::Binding,
    selection: super::super::super::code_panel::selection::SourceSelection,
    pub(super) captured: crate::syndic_transcript::CapturedDiscussionSource,
}

fn raw_fragments(
    page: &crate::syndic_transcript::provider::IndexedResourceResponse,
) -> Vec<(
    &crate::syndic_transcript::provider::resource_index::Binding,
    crate::syndic_transcript::provider::resource_index::ByteRange,
    &[u8],
)> {
    use crate::syndic_transcript::provider::IndexedResourceValue;
    match &page.value {
        IndexedResourceValue::Source(source) => {
            vec![(&source.binding, source.range, source.bytes.as_slice())]
        }
        IndexedResourceValue::Display(display) => display
            .fragment
            .iter()
            .map(|fragment| (&display.binding, fragment.source, fragment.text.as_bytes()))
            .collect(),
        IndexedResourceValue::Range(range) => range
            .presentations
            .iter()
            .flatten()
            .filter_map(|presentation| presentation.fragment.as_ref())
            .map(|fragment| (&range.binding, fragment.source, fragment.text.as_bytes()))
            .collect(),
        _ => Vec::new(),
    }
}

impl SyndicTranscriptPanel {
    fn capture_code_discussion(&self, id: u64) -> Option<CodeDiscussionCapture> {
        let code = self
            .code_panels
            .iter()
            .find(|code| code.id == id && !code.overridden)?;
        let binding = code.layout.source_binding()?;
        if code.layout.state(&code.viewport) != "ready" {
            return None;
        }
        let root = code.layout.source_root()?;
        if code.actions.binding.as_ref() != Some(binding) || code.actions.root != Some(root) {
            return None;
        }
        let selection = code.actions.selection?;
        let range = code.actions.selected_range()?;
        let length = range.end.checked_sub(range.start)?;
        if length == 0
            || length > 65_536
            || range.start < binding.range.start
            || range.end > binding.range.end
            || binding.resource.id.to_string() != code.viewport.owner.resource_id.0
            || Some(binding.metadata.item_id().to_string()).as_deref()
                != code
                    .viewport
                    .owner
                    .provenance
                    .item_id
                    .as_ref()
                    .map(|item| item.0.as_str())
            || binding
                .metadata
                .projection_id()
                .map(|projection| projection.to_string())
                .as_deref()
                != code
                    .viewport
                    .owner
                    .provenance
                    .projection_id
                    .as_ref()
                    .map(|projection| projection.0.as_str())
        {
            return None;
        }
        let mut rendered: Vec<_> = code
            .layout
            .prepared()
            .flat_map(|prepared| &prepared.fragments)
            .filter(|fragment| fragment.raw_source.is_some())
            .map(|fragment| fragment.source.clone())
            .collect();
        rendered.sort_by_key(|range| range.start);
        rendered.extend(
            code.layout
                .source_separators()
                .map(|(range, _)| range.start..range.end),
        );
        rendered.sort_by_key(|range| range.start);
        let mut cursor = range.start;
        for fragment in rendered {
            if fragment.end <= cursor {
                continue;
            }
            if fragment.start > cursor {
                return None;
            }
            cursor = fragment.end.min(range.end);
            if cursor == range.end {
                break;
            }
        }
        if cursor != range.end {
            return None;
        }
        let pages = self
            .host
            .indexed_resource_pages(&code.viewport.owner.resource_id);
        let mut bytes = Vec::with_capacity(usize::try_from(length).ok()?);
        let mut cursor = range.start;
        while cursor < range.end {
            let source = pages
                .iter()
                .chain(code.layout.source_seeds())
                .filter(|page| page.root.as_ref().is_some_and(|page| page.digest == root))
                .flat_map(|page| raw_fragments(page))
                .filter(|(current_binding, source, bytes)| {
                    *current_binding == binding
                        && source.start <= cursor
                        && source.end > cursor
                        && source
                            .end
                            .checked_sub(source.start)
                            .is_some_and(|length| bytes.len() as u64 == length)
                })
                .map(|(_, range, bytes)| (range, bytes))
                .chain(
                    code.layout
                        .prepared()
                        .flat_map(|prepared| &prepared.fragments)
                        .filter_map(|fragment| {
                            let raw = fragment.raw_source.as_deref()?;
                            (fragment.source.start <= cursor
                                && fragment.source.end > cursor
                                && raw.len() as u64
                                    == fragment.source.end.checked_sub(fragment.source.start)?)
                            .then_some((
                                crate::syndic_transcript::provider::resource_index::ByteRange {
                                    start: fragment.source.start,
                                    end: fragment.source.end,
                                },
                                raw.as_bytes(),
                            ))
                        }),
                )
                .chain(
                    code.layout
                        .source_separators()
                        .filter(|(range, _)| range.start <= cursor && range.end > cursor),
                )
                .max_by_key(|(range, _)| range.end)?;
            let end = source.0.end.min(range.end);
            let start_index = usize::try_from(cursor.checked_sub(source.0.start)?).ok()?;
            let end_index = usize::try_from(end.checked_sub(source.0.start)?).ok()?;
            bytes.extend_from_slice(source.1.get(start_index..end_index)?);
            cursor = end;
        }
        let snapshot = self.host.shared_snapshot();
        let captured = self.host.resource_discussion_source(
            &code.viewport.owner,
            snapshot.activation_revision,
            snapshot.presentation_revision,
            range,
            String::from_utf8(bytes).ok()?,
        )?;
        Some(CodeDiscussionCapture {
            id,
            owner: super::super::super::code_panel::source_actions::identity(&code.viewport.owner),
            activation: snapshot.activation_revision,
            presentation: snapshot.presentation_revision,
            root,
            binding: binding.clone(),
            selection,
            captured,
        })
    }

    pub(super) fn code_discussion_request_current(
        &self,
        request: u64,
        activation: u64,
        presentation: u64,
        window: &Window,
    ) -> bool {
        let Some(capture) = &self.quote_state.code_capture else {
            return false;
        };
        let current = self.host.shared_snapshot();
        request == self.quote_state.serial
            && self
                .code_panels
                .iter()
                .find(|code| code.id == capture.id)
                .is_some_and(|code| {
                    code.layout
                        .geometry_current(&code.viewport, &self.code_appearance, window)
                })
            && current.activation_revision == activation
            && current.presentation_revision == presentation
            && capture.activation == activation
            && capture.presentation == presentation
            && self.capture_code_discussion(capture.id).is_some_and(|now| {
                now.captured == capture.captured
                    && now.binding == capture.binding
                    && now.root == capture.root
                    && now.selection == capture.selection
            })
    }

    pub(in super::super::super) fn open_code_discussion_menu(
        &mut self,
        id: u64,
        pointer: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(capture) = self.capture_code_discussion(id) else {
            return;
        };
        let Some(code) = self.code_panels.iter().find(|code| code.id == id) else {
            return;
        };
        if !code
            .layout
            .geometry_current(&code.viewport, &self.code_appearance, window)
        {
            return;
        }
        if pointer.is_none() && !code.focus.is_focused(window) {
            return;
        }
        if let Some(pointer) = pointer {
            if !super::super::super::code_panel::selection::hit(
                &code.layout,
                &code.viewport,
                pointer,
            )
            .is_some_and(|hit| {
                capture.selection.range().start <= hit && hit <= capture.selection.range().end
            }) {
                return;
            }
        }
        let anchor = pointer
            .unwrap_or(code.viewport.scroll.borrow().bounds.origin + point(px(10.), px(10.)));
        self.close_quote_menu(window);
        self.invalidate_transcript_menu();
        let Some(serial) = self.quote_state.serial.checked_add(1) else {
            return;
        };
        self.quote_state.serial = serial;
        self.quote_state.code_capture = Some(capture.clone());
        self.quote_state.anchor = Some(Bounds::new(anchor, size(px(1.), px(1.))));
        let rows = self.transcript_menu_rows();
        let mut popup = AnchoredContextMenu::open(serial, window, cx);
        popup.reconcile(&rows);
        popup.realize(
            &(0..rows.len())
                .map(|index| rows.row(index))
                .collect::<Vec<_>>(),
            0..rows.len(),
            window,
        );
        self.quote_state.popup = Some(popup);
        cx.emit(
            crate::syndic_transcript::DiscussionSourceQualificationRequested(
                crate::syndic_transcript::DiscussionCreationRequested {
                    captured: capture.captured,
                    request: serial,
                    activation: capture.activation,
                    presentation: capture.presentation,
                },
            ),
        );
        cx.notify();
    }

    pub(super) fn activate_code_discussion(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(capture) = self.quote_state.code_capture.clone() else {
            return;
        };
        if !self.quote_request_current(
            self.quote_state.serial,
            capture.activation,
            capture.presentation,
            window,
        ) {
            return;
        }
        self.quote_state.preserved_code_selection = Some(capture.clone());
        self.quote_state.preserved_branch_selection = None;
        self.quote_state.restore_branch_selection = false;
        let retry = self.quote_state.branch_retry
            && self.quote_state.branch_source.as_ref() == Some(&capture.captured);
        let request = crate::syndic_transcript::DiscussionCreationRequested {
            captured: capture.captured,
            request: self.quote_state.serial,
            activation: capture.activation,
            presentation: capture.presentation,
        };
        if retry {
            cx.emit(crate::syndic_transcript::DiscussionActivationRetryRequested(request));
        } else {
            cx.emit(request);
        }
    }

    pub(super) fn quote_code_discussion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self.quote_state.code_capture.clone() else {
            return;
        };
        if !self.quote_request_current(
            self.quote_state.serial,
            capture.activation,
            capture.presentation,
            window,
        ) {
            return;
        }
        let request = crate::syndic_transcript::QuoteInsertionRequested {
            markdown: capture.captured.text.as_str().to_owned(),
            request: self.quote_state.serial,
            activation: capture.activation,
            presentation: capture.presentation,
            cancellation: beryl_home_store::CommandCancellation::new(),
        };
        self.close_quote_menu(window);
        cx.emit(request);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn code_discussion_origin_for_test(
        &self,
        window: &Window,
    ) -> Option<(bool, bool, bool)> {
        let code = self.code_panels.first()?;
        Some((
            code.focus.is_focused(window),
            code.layout
                .geometry_current(&code.viewport, &self.code_appearance, window),
            self.host
                .indexed_resource_owner(&code.viewport.owner.resource_id)
                .is_some(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn code_discussion_capture_ready_for_test(&self) -> bool {
        self.code_panels
            .first()
            .is_some_and(|code| self.capture_code_discussion(code.id).is_some())
    }

    #[cfg(test)]
    pub(crate) fn code_discussion_facts_for_test(&self) -> Option<String> {
        let code = self.code_panels.first()?;
        let raw: Vec<_> = code
            .layout
            .prepared()
            .flat_map(|prepared| &prepared.fragments)
            .map(|fragment| {
                (
                    fragment.source.clone(),
                    fragment.raw_source.as_ref().map(|raw| raw.len()),
                    fragment.oversize,
                )
            })
            .collect();
        Some(format!(
            "selection={:?} state={} binding={} root={} owner_presentation={} invocation_presentation={} raw={raw:?} capture={}",
            code.actions.selected_range(),
            code.layout.state(&code.viewport),
            code.actions.binding.as_ref() == code.layout.source_binding(),
            code.actions.root == code.layout.source_root(),
            code.viewport.owner.presentation_revision,
            self.host.shared_snapshot().presentation_revision,
            self.capture_code_discussion(code.id).is_some()
        ))
    }

    pub(in super::super::super) fn restore_code_discussion_selection(
        &mut self,
        window: &mut Window,
    ) {
        if !(self.quote_state.branch_pending || self.quote_state.restore_branch_selection) {
            return;
        }
        let Some(saved) = self.quote_state.preserved_code_selection.clone() else {
            return;
        };
        let Some(code) = self.code_panels.iter_mut().find(|code| {
            !code.overridden
                && code.viewport.owner.resource_id == saved.owner.resource_id
                && code.layout.source_binding() == Some(&saved.binding)
                && code.layout.source_root() == Some(saved.root)
        }) else {
            return;
        };
        let previous = code.actions.selection;
        code.actions.selection = Some(saved.selection);
        let id = code.id;
        let focus = code.focus.clone();
        if self
            .capture_code_discussion(id)
            .is_some_and(|current| current.captured == saved.captured)
        {
            if self.quote_state.restore_branch_selection {
                window.focus(&focus);
            }
            self.quote_state.restore_branch_selection = false;
        } else if let Some(code) = self.code_panels.iter_mut().find(|code| code.id == id) {
            code.actions.selection = previous;
        }
    }
}
