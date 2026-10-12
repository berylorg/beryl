use super::*;

impl SyndicTranscriptPanel {
    pub(crate) fn code_save_unavailable_for_test(&self) -> Option<&'static str> {
        super::save::unavailable(self.code_panels.first()?, &self.host, self.staged.is_some())
    }
    pub(crate) fn code_hold_save_writer_for_test(
        &mut self,
        publish: bool,
    ) -> std::sync::Arc<super::save::SaveWriterGate> {
        let gate = super::save::SaveWriterGate::new(publish);
        self.code_panels.first_mut().unwrap().actions.save_hold = Some(gate.clone());
        gate
    }
    pub(crate) fn code_source_action_charge_for_test(&self) -> usize {
        self.code_panels
            .first()
            .map_or(0, |code| code.actions.charge())
    }
    pub(crate) fn code_save_geometry_for_test(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.code_panels.first()?.save_bounds.get()
    }
    pub(crate) fn code_set_save_choice_for_test(
        &mut self,
        choice: Result<Option<gpui::SavePathChoice>, ()>,
    ) {
        let sender = self.code_defer_save_choice_for_test();
        let _ = sender.send(choice);
    }
    pub(crate) fn code_defer_save_choice_for_test(
        &mut self,
    ) -> futures_channel::oneshot::Sender<Result<Option<gpui::SavePathChoice>, ()>> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        self.code_panels.first_mut().unwrap().actions.save_choice = Some(receiver);
        sender
    }
    pub(crate) fn code_save_pending_for_test(&self) -> bool {
        self.code_panels
            .first()
            .is_some_and(|code| code.actions.save.is_some())
    }
    pub(crate) fn code_set_save_fault_for_test(&mut self, publication: bool) {
        self.code_panels.first_mut().unwrap().actions.save_fault = Some(if publication {
            super::save::SaveFault::Publish
        } else {
            super::save::SaveFault::Write
        });
    }
    pub(crate) fn code_save_fault_pending_for_test(&self) -> bool {
        self.code_panels
            .first()
            .is_some_and(|code| code.actions.save_fault.is_some())
    }
    pub(crate) fn code_layout_work_for_test(&self) -> Option<String> {
        let code = self.code_panels.first()?;
        let scroll = code.viewport.scroll.borrow();
        Some(format!(
            "{}; top={} height={} inline_remap={} source_remap={} anchor={:?}",
            code.layout.work_status(),
            scroll.top_line,
            scroll.line_height,
            code.viewport.inline_remapping,
            code.viewport.remapping,
            code.viewport
                .inline_anchor
                .map(|anchor| (anchor.logical_line, anchor.source.position))
        ))
    }
    pub(crate) fn code_command_geometry_for_test(
        &self,
    ) -> Option<(
        gpui::Bounds<gpui::Pixels>,
        gpui::Bounds<gpui::Pixels>,
        gpui::Bounds<gpui::Pixels>,
    )> {
        let code = self.code_panels.first()?;
        let (footer, mask) = code.feedback_bounds.get()?;
        Some((code.command_bounds.get()?, footer, mask))
    }
    pub(crate) fn code_copy_unavailable_for_test(&self) -> Option<&'static str> {
        let code = self.code_panels.first()?;
        super::commands::whole_copy_unavailable(code, &self.host, self.staged.is_some())
    }
    pub(crate) fn code_selection_painted_quads_for_test(&self) -> usize {
        self.code_panels
            .first()
            .map_or(0, |code| code.painted_selection_quads.get())
    }
    pub(crate) fn code_commands_current_for_test(&self) -> bool {
        self.staged.is_none()
    }
    pub(crate) fn code_line_height_for_test(&self) -> Option<f32> {
        Some(
            self.code_panels
                .first()?
                .viewport
                .scroll
                .borrow()
                .line_height,
        )
    }
    pub(crate) fn code_selection_for_test(&self) -> Option<std::ops::Range<u64>> {
        self.code_panels
            .first()?
            .actions
            .selection
            .map(super::selection::SourceSelection::range)
    }
    pub(crate) fn code_copy_feedback_for_test(&self) -> Option<&'static str> {
        Some(self.code_panels.first()?.actions.feedback)
    }
    pub(crate) fn code_copy_pending_for_test(&self) -> bool {
        self.code_panels
            .first()
            .is_some_and(|code| code.actions.copy.is_some())
    }
    pub(crate) fn code_copy_progress_for_test(&self) -> Option<(std::ops::Range<u64>, usize)> {
        self.code_panels.first()?.actions.copy_progress()
    }
    pub(crate) fn code_set_clipboard_writer_for_test(
        &mut self,
        writer: impl FnMut(
            &gpui::ClipboardItem,
            gpui::ClipboardLimits,
            &mut gpui::App,
        ) -> Result<(), gpui::ClipboardError>
        + 'static,
    ) {
        self.code_panels.first_mut().unwrap().actions.writer = Some(Box::new(writer));
    }
    pub(crate) fn code_source_point_for_test(
        &self,
        source: u64,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let code = self.code_panels.first()?;
        let scroll = code.viewport.scroll.borrow();
        for prepared in code.layout.prepared() {
            for segment in &prepared.fragments {
                let top = (segment.logical_line as i128 - scroll.top_line as i128) as f32
                    * scroll.line_height
                    - scroll.remainder;
                for fragment in segment.fragments.iter() {
                    if let Some(x) = super::selection::source_x(fragment, source) {
                        let point = scroll.bounds.origin
                            + gpui::point(
                                x + gpui::px(10.0) - prepared.target.inline_start,
                                gpui::px(top + scroll.line_height * 0.5),
                            );
                        if scroll.bounds.contains(&point) {
                            return Some(point);
                        }
                    }
                }
            }
        }
        None
    }
    pub(crate) fn code_source_atoms_for_test(
        &self,
    ) -> Vec<(std::ops::Range<u64>, gpui::Bounds<gpui::Pixels>)> {
        let Some(code) = self.code_panels.first() else {
            return Vec::new();
        };
        let scroll = code.viewport.scroll.borrow();
        let mut atoms = Vec::new();
        for prepared in code.layout.prepared() {
            for segment in &prepared.fragments {
                let top = (segment.logical_line as i128 - scroll.top_line as i128) as f32
                    * scroll.line_height
                    - scroll.remainder;
                for fragment in segment.fragments.iter() {
                    if let gpui::StreamingLayoutFragment::OversizeAtom(atom) = fragment {
                        let maps = atom.maps();
                        let origin = scroll.bounds.origin
                            + gpui::point(
                                maps[0].position.x + gpui::px(10.0) - prepared.target.inline_start,
                                gpui::px(top),
                            );
                        atoms.push((
                            maps[0].logical_position.byte_offset
                                ..maps[1].logical_position.byte_offset,
                            gpui::Bounds::new(
                                origin,
                                gpui::size(
                                    maps[1].position.x - maps[0].position.x,
                                    gpui::px(scroll.line_height),
                                ),
                            ),
                        ));
                    }
                }
            }
        }
        atoms
    }
    pub(crate) fn code_complete_line_width_for_test(&self, ordinal: u64) -> Option<f32> {
        self.code_panels.first()?.layout.line_width(ordinal)
    }

    pub(crate) fn code_foreground_response_for_test(
        &self,
        resource: &super::super::super::ResourceId,
    ) -> Option<std::sync::Arc<super::super::super::IndexedResourceResponse>> {
        let mounted = self
            .code_panels
            .iter()
            .find(|panel| &panel.viewport.owner.resource_id == resource)?;
        self.host
            .indexed_resource_response(resource)
            .filter(|response| matches!(response.value, IndexedResourceValue::Range(_)))
            .or_else(|| mounted.layout.foreground_response())
    }

    pub(crate) fn code_inline_anchor_for_test(&self) -> Option<(u64, u64, f32)> {
        let anchor = self.code_panels.first()?.viewport.inline_anchor?;
        Some((
            anchor.logical_line,
            anchor.source.position,
            anchor.visual_offset,
        ))
    }

    pub(crate) fn code_inline_anchor_painted_position_for_test(&self) -> Option<f32> {
        let panel = self.code_panels.first()?;
        let anchor = panel.viewport.inline_anchor?;
        for prepared in panel.layout.prepared() {
            for fragment in &prepared.fragments {
                if fragment.logical_line != anchor.logical_line {
                    continue;
                }
                for fragment in fragment.fragments.iter() {
                    let position = gpui::StreamingLayoutPosition::at(anchor.source.position);
                    let point = match fragment {
                        gpui::StreamingLayoutFragment::Text(fragment) => fragment
                            .position_for_logical_position(position)
                            .ok()
                            .flatten(),
                        gpui::StreamingLayoutFragment::OversizeAtom(fragment) => {
                            fragment.position_for_logical_position(position)
                        }
                        _ => None,
                    };
                    if let Some(point) = point {
                        return Some(f32::from(point.x - prepared.target.inline_start));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn code_painted_body_glyphs_for_test(&self) -> usize {
        self.code_panels
            .first()
            .map_or(0, |panel| panel.painted_body_glyphs.get())
    }
    pub(crate) fn code_layout_line_source_range_for_test(
        &self,
        ordinal: u64,
    ) -> Option<std::ops::Range<u64>> {
        self.code_panels
            .first()?
            .layout
            .prepared()
            .flat_map(|prepared| &prepared.fragments)
            .filter(|fragment| fragment.logical_line == ordinal)
            .fold(None, |range: Option<std::ops::Range<u64>>, fragment| {
                Some(match range {
                    Some(range) => {
                        range.start.min(fragment.source.start)..range.end.max(fragment.source.end)
                    }
                    None => fragment.source.clone(),
                })
            })
    }

    pub(crate) fn code_tab_stop_for_test(&self, window: &gpui::Window) -> f32 {
        let run = gpui::TextRun {
            len: 1,
            font: self.code_appearance.body_font.font.clone(),
            color: self.code_appearance.body_text.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        f32::from(
            window
                .text_system()
                .shape_line(
                    " ".into(),
                    px(self.code_appearance.body_font.size),
                    &[run],
                    None,
                )
                .width,
        ) * 8.0
    }

    pub(crate) fn code_layout_first_fragment_end_for_test(&self, ordinal: u64) -> Option<f32> {
        self.code_panels
            .first()?
            .layout
            .prepared()
            .flat_map(|prepared| &prepared.fragments)
            .find(|fragment| fragment.logical_line == ordinal)
            .map(|fragment| f32::from(fragment.inline_end))
    }

    pub(crate) fn code_layout_painted_offsets_for_test(&self) -> Vec<f32> {
        self.code_panels
            .first()
            .into_iter()
            .flat_map(|panel| panel.layout.prepared())
            .map(|prepared| f32::from(prepared.target.inline_start))
            .collect()
    }

    pub(crate) fn code_layout_max_fragment_bytes_for_test(&self) -> Option<u64> {
        self.code_panels
            .first()?
            .layout
            .prepared()
            .flat_map(|prepared| &prepared.fragments)
            .filter(|fragment| !fragment.oversize)
            .map(|fragment| fragment.source.end - fragment.source.start)
            .max()
    }

    pub(crate) fn pause_code_reads_through_revision_for_test(
        &mut self,
        revision: super::super::super::ProviderRevision,
    ) {
        self.code_reads_paused_through_revision = Some(revision);
    }

    pub(crate) fn code_provider_revision_for_test(
        &self,
    ) -> Option<super::super::super::ProviderRevision> {
        Some(self.code_panels.first()?.viewport.owner.revision)
    }

    pub(crate) fn code_horizontal_offset_for_test(&self) -> Option<f32> {
        Some(
            self.code_panels
                .first()?
                .viewport
                .scroll
                .borrow()
                .horizontal,
        )
    }

    pub(crate) fn code_anchor_for_test(&self) -> Option<index::SourceAnchor> {
        self.code_panels.first()?.viewport.anchor
    }

    pub(crate) fn override_code_owner_for_test(
        &mut self,
        resource: &super::super::super::ResourceId,
    ) -> std::rc::Weak<dyn std::any::Any> {
        let mounted = self
            .code_panels
            .iter()
            .find(|panel| &panel.viewport.owner.resource_id == resource)
            .unwrap();
        let scroll: Rc<dyn std::any::Any> = mounted.viewport.scroll.clone();
        let released = Rc::downgrade(&scroll);
        let owner = self.host.indexed_resource_owner(resource).unwrap();
        let query = mounted.viewport.query();
        assert!(self.set_indexed_resource_demand(&owner, Some(query)));
        released
    }
    pub(crate) fn code_scrollbar_for_test(
        &self,
    ) -> Option<gpui_scrollbar::ScrollbarGeometrySnapshot> {
        self.code_panels
            .first()?
            .interaction
            .current_snapshot(Axis::Vertical, self.code_appearance.scrollbar)
    }

    pub(crate) fn code_presentation_for_test(&self) -> Option<(u64, f32, f32)> {
        let panel = self.code_panels.first()?;
        let scroll = panel.viewport.scroll.borrow();
        Some((panel.id, scroll.line_height, scroll.remainder))
    }

    pub(crate) fn code_viewport_for_test(
        &self,
    ) -> Option<(
        super::super::super::ResourceId,
        gpui::Bounds<gpui::Pixels>,
        u64,
        bool,
    )> {
        let panel = self.code_panels.first()?;
        let scroll = panel.viewport.scroll.borrow();
        Some((
            panel.viewport.owner.resource_id.clone(),
            scroll.bounds,
            scroll.top_line,
            self.code_wheel_owner.get() == Some(panel.id),
        ))
    }

    pub(crate) fn code_viewports_for_test(
        &self,
    ) -> Vec<(
        super::super::super::ResourceId,
        gpui::Bounds<gpui::Pixels>,
        u64,
        bool,
    )> {
        self.code_panels
            .iter()
            .map(|panel| {
                let scroll = panel.viewport.scroll.borrow();
                (
                    panel.viewport.owner.resource_id.clone(),
                    scroll.bounds,
                    scroll.top_line,
                    self.code_wheel_owner.get() == Some(panel.id),
                )
            })
            .collect()
    }
}
