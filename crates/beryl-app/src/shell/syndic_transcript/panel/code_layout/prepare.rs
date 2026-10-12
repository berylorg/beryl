use super::*;

impl CodeLayoutIndex {
    pub(super) fn advance_inner(
        &mut self,
        text_system: &WindowTextSystem,
        run: &TextRun,
    ) -> Result<CodeLayoutProgress, CodeLayoutError> {
        if self.complete {
            return Ok(CodeLayoutProgress::Complete);
        }
        if self.ready {
            return Ok(CodeLayoutProgress::Ready);
        }
        let mut steps = self.limits.boundary_steps_per_quantum;
        for _ in 0..self.limits.segments_per_quantum {
            self.check_capacity(0, 0)?;
            let Some(segment) = self.scan.next(&mut steps)? else {
                break;
            };
            let mut session = match self.continuation {
                Some(continuation) => text_system.resume_streaming_layout_session_with_mode(
                    self.binding.clone(),
                    continuation,
                    StreamingLayoutMode::NoWrap,
                )?,
                None => text_system.streaming_layout_session_with_mode(
                    self.binding.clone(),
                    StreamingLayoutMode::NoWrap,
                )?,
            };
            let prior = session
                .continuation()
                .ok_or(CodeLayoutError::InvalidGeometry)?;
            let is_terminal = matches!(&segment.kind, SegmentKind::Delimiter | SegmentKind::End);
            let is_end = matches!(&segment.kind, SegmentKind::End);
            let oversize = matches!(&segment.kind, SegmentKind::Oversize);
            let admission = match &segment.kind {
                SegmentKind::Text(text) => {
                    let mut run = run.clone();
                    run.len = text.len();
                    session.admit_text(StreamingTextSegment {
                        input_id: self.binding.input_id,
                        segment_policy_id: self.binding.segment_policy_id,
                        ordinal: prior.next_ordinal,
                        logical_range: logical_range(&segment.source),
                        text: text.clone().into(),
                        runs: vec![run],
                    })?
                }
                SegmentKind::Tab | SegmentKind::Oversize => {
                    let stop = self.tab_stop(text_system, run)?;
                    let offset = f32::from(prior.inline_offset);
                    let width = if oversize {
                        stop
                    } else {
                        tab_advance(prior.inline_offset, stop)?
                    };
                    if !offset.is_finite() {
                        return Err(CodeLayoutError::InvalidGeometry);
                    }
                    let label = if oversize { "?" } else { "" };
                    let mut run = run.clone();
                    run.len = label.len();
                    let (height, baseline) = if oversize {
                        let shaped = text_system
                            .layout_line_checked(
                                label,
                                self.binding.font_size,
                                &[run.clone()],
                                None,
                            )
                            .map_err(checked_shaping_error)?;
                        (
                            self.binding.line_height.max(shaped.ascent + shaped.descent),
                            shaped.ascent,
                        )
                    } else {
                        (self.binding.line_height, Pixels::ZERO)
                    };
                    session.admit_oversize_atom(StreamingOversizeAtom {
                        input_id: self.binding.input_id,
                        segment_policy_id: self.binding.segment_policy_id,
                        ordinal: prior.next_ordinal,
                        logical_range: logical_range(&segment.source),
                        presentation: label.into(),
                        runs: if label.is_empty() {
                            Vec::new()
                        } else {
                            vec![run]
                        },
                        width,
                        height,
                        baseline,
                        background: None,
                    })?
                }
                SegmentKind::Delimiter => {
                    session.finalize_logical_line(StreamingLineFinalization {
                        input_id: self.binding.input_id,
                        segment_policy_id: self.binding.segment_policy_id,
                        ordinal: prior.next_ordinal,
                        delimiter_range: Some(logical_range(&segment.source)),
                        next_position: StreamingLayoutPosition::at(segment.source.end),
                    })?
                }
                SegmentKind::End => session.end_source(StreamingEndOfSource {
                    input_id: self.binding.input_id,
                    segment_policy_id: self.binding.segment_policy_id,
                    ordinal: prior.next_ordinal,
                    source_extent: self.source.end,
                    position: StreamingLayoutPosition::at(self.source.end),
                })?,
            };
            drop(session);
            let end = if is_terminal {
                prior.inline_offset
            } else {
                admission.continuation.inline_offset
            };
            self.resolve_anchor(&segment, prior.inline_offset, &admission)?;
            if is_terminal {
                self.max_width = self.max_width.max(end);
                if self
                    .target
                    .as_ref()
                    .is_some_and(|target| self.line == target.lines.end - 1)
                {
                    self.terminal_width = Some(end);
                }
            } else {
                self.retain_fragment(&segment, prior.inline_offset, end, oversize, &admission)?;
            }
            self.continuation = Some(admission.continuation);
            if is_terminal && !is_end {
                self.line = self.line.checked_add(1).ok_or(CodeLayoutError::Overflow)?;
            }
            if is_end {
                self.complete = true;
                self.complete_max_width = Some(self.max_width);
            }
            self.add_checkpoint(segment.checkpoint, admission.continuation)?;
            if let Some(target) = &self.target {
                self.ready = self.complete
                    || self.line >= target.lines.end
                    || (!is_terminal
                        && self.line == target.lines.end - 1
                        && end >= target.inline_end);
            }
            if self.resolved_anchor.is_some() {
                self.ready = true;
            }
            if self.complete && self.anchor.is_some() && self.resolved_anchor.is_none() {
                return Err(CodeLayoutError::InvalidTarget);
            }
            if self.ready || self.complete {
                break;
            }
        }
        Ok(if self.complete {
            CodeLayoutProgress::Complete
        } else if self.ready {
            CodeLayoutProgress::Ready
        } else if self.scan.read().is_some() {
            CodeLayoutProgress::NeedsSource
        } else {
            CodeLayoutProgress::Pending
        })
    }

    fn resolve_anchor(
        &mut self,
        segment: &Segment,
        start: Pixels,
        admission: &StreamingLayoutAdmission,
    ) -> Result<(), CodeLayoutError> {
        let Some(anchor) = self.anchor else {
            return Ok(());
        };
        if self.line != anchor.logical_line
            || anchor.source < segment.source.start
            || anchor.source >= segment.source.end && !matches!(segment.kind, SegmentKind::End)
        {
            return Ok(());
        }
        let position = StreamingLayoutPosition::at(anchor.source);
        let inline = if matches!(segment.kind, SegmentKind::End | SegmentKind::Delimiter)
            && anchor.source == segment.source.start
        {
            Some(start)
        } else {
            let mut value = None;
            for fragment in admission.fragments.iter() {
                value = match fragment {
                    StreamingLayoutFragment::Text(text) => text
                        .position_for_logical_position(position)?
                        .map(|point| point.x),
                    StreamingLayoutFragment::OversizeAtom(atom) => atom
                        .position_for_logical_position(position)
                        .map(|point| point.x),
                    _ => None,
                };
                if value.is_some() {
                    break;
                }
            }
            value
        }
        .ok_or(CodeLayoutError::InvalidGeometry)?;
        self.resolved_anchor = Some(ResolvedCodeAnchor { anchor, inline });
        Ok(())
    }

    fn tab_stop(
        &mut self,
        text_system: &WindowTextSystem,
        run: &TextRun,
    ) -> Result<Pixels, CodeLayoutError> {
        if let Some(advance) = self.space_advance {
            return Ok(advance * 8.0);
        }
        let mut run = run.clone();
        run.len = 1;
        let advance = text_system
            .layout_line_checked(" ", self.binding.font_size, &[run], None)
            .map_err(checked_shaping_error)?
            .width;
        let stop = f32::from(advance) * 8.0;
        if !stop.is_finite() || stop <= 0.0 {
            return Err(CodeLayoutError::InvalidGeometry);
        }
        self.space_advance = Some(advance);
        Ok(px(stop))
    }

    fn retain_fragment(
        &mut self,
        segment: &Segment,
        start: Pixels,
        end: Pixels,
        oversize: bool,
        admission: &StreamingLayoutAdmission,
    ) -> Result<(), CodeLayoutError> {
        let Some(target) = &self.target else {
            return Ok(());
        };
        if !target.lines.contains(&self.line)
            || end < target.inline_start
            || start > target.inline_end
        {
            return Ok(());
        }
        if self.output.len() == self.limits.fragments {
            return Err(CodeLayoutError::Capacity);
        }
        let bytes = admission.charge.total()?;
        let items = admission.item_charge.total()?;
        self.check_capacity(bytes, items)?;
        self.output_bytes = self
            .output_bytes
            .checked_add(bytes)
            .ok_or(CodeLayoutError::Overflow)?;
        self.output_items = self
            .output_items
            .checked_add(items)
            .ok_or(CodeLayoutError::Overflow)?;
        self.output.push(PreparedCodeFragment {
            logical_line: self.line,
            source: segment.source.clone(),
            inline_start: start,
            inline_end: end,
            fragments: admission.fragments.clone(),
            oversize,
        });
        Ok(())
    }
}
