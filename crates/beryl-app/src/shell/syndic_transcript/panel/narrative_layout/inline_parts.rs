use super::*;
use super::super::{
    ResidentPresentationRecord, ResidentPresentationRecordKind,
    narrative_appearance::{NarrativeAppearance, NarrativeBlockAppearance, NarrativeInlineStyle},
};
use gpui::{StreamingInlineObject, StreamingObjectGap, StreamingObjectId, StreamingObjectOrder};

pub(in super::super) struct InlinePartsLayout {
    pub(in super::super) text: Vec<(usize, Arc<NarrativeLayout>)>,
    pub(in super::super) markers: Vec<(usize, Bounds<Pixels>)>,
    pub(in super::super) height: Pixels,
    pub(in super::super) charge: usize,
}

pub(in super::super) struct InlinePartsPreparation {
    stream: NarrativeLayoutPreparation,
    part: usize,
    fragment: usize,
    base: usize,
    started: usize,
    spans: Vec<(usize, usize, Range<usize>)>,
}

fn object(record: &ResidentPresentationRecord) -> Option<(StreamingObjectId, StreamingObjectOrder)> {
    let ResidentPresentationRecordKind::ImageMarker(marker) = record.kind else { return None };
    Some((StreamingObjectId(u128::from_be_bytes(*marker.marker.marker_id().as_bytes())),
        StreamingObjectOrder(u128::from(marker.marker_ordinal.get()))))
}

fn next_object(parts: &[ResidentPresentationRecord], start: usize) -> Option<(StreamingObjectId, StreamingObjectOrder)> {
    for part in parts.get(start..)? {
        if let Some(object) = object(part) { return Some(object); }
        if !matches!(&part.kind, ResidentPresentationRecordKind::NarrativeReady { prepared } if prepared.display().is_empty()) {
            return None;
        }
    }
    None
}

impl InlinePartsPreparation {
    pub(in super::super) fn new(
        parts: &[ResidentPresentationRecord], revision: u64, width: Pixels,
        block: &NarrativeBlockAppearance, allowance: usize,
    ) -> Self {
        let mut stream = NarrativeLayoutPreparation::new(revision, width, px(block.text.size),
            px(block.text.size * 20.0 / 14.0), allowance);
        if let Some((id, order)) = next_object(parts, 0) {
            stream.binding.start_position = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(id, order));
        }
        Self { stream, part: 0, fragment: 0, base: 0, started: 0, spans: Vec::new() }
    }

    pub(in super::super) fn charge(&self) -> usize {
        self.stream.charge().saturating_add(self.spans.capacity() * size_of::<(usize, usize, Range<usize>)>())
    }

    pub(in super::super) fn advance(
        &mut self, parts: &[ResidentPresentationRecord], appearance: &NarrativeAppearance,
        block: &NarrativeBlockAppearance, window: &Window, observations: &mut usize,
    ) -> Result<Option<InlinePartsLayout>, StreamingLayoutError> {
        while self.part < parts.len() {
            if *observations == 0 { return Ok(None); }
            *observations -= 1;
            match &parts[self.part].kind {
                ResidentPresentationRecordKind::NarrativeReady { prepared } => {
                    if let Some(fragment) = prepared.fragments().get(self.fragment) {
                        let mut cursor = fragment.display.start;
                        while cursor < fragment.display.end {
                            let newline = prepared.display()[cursor..fragment.display.end].find('\n').map(|offset| cursor + offset);
                            let end = newline.unwrap_or(fragment.display.end);
                            let terminal = end == prepared.display().len();
                            let next = terminal.then(|| next_object(parts, self.part + 1)).flatten();
                            let position = next.map_or_else(
                                || StreamingLayoutPosition::at((self.base + end) as u64),
                                |(id, order)| StreamingLayoutPosition::with_gap((self.base + end) as u64, StreamingObjectGap::before(id, order)),
                            );
                            if cursor < end {
                                let runs = prepared.runs()[fragment.runs.clone()].iter().filter_map(|run| {
                                    let start = run.display.start.max(cursor);
                                    let stop = run.display.end.min(end);
                                    (start < stop).then(|| appearance.inline(block, NarrativeInlineStyle {
                                        emphasis: run.style.emphasis, strong: run.style.strong,
                                        strikethrough: run.style.strikethrough, code: run.style.code,
                                        link_label: run.style.link_label,
                                    }).run(stop - start))
                                }).collect();
                                let (ordinal, start) = self.stream.next_input();
                                let admission = self.stream.session(window.text_system())?.admit_text(StreamingTextSegment {
                                    input_id: self.stream.binding.input_id,
                                    segment_policy_id: self.stream.binding.segment_policy_id,
                                    ordinal, logical_range: start..position,
                                    text: prepared.display()[cursor..end].to_owned().into(), runs,
                                })?;
                                self.stream.retain(admission)?;
                            }
                            cursor = end;
                            if newline.is_some() {
                                let (ordinal, start) = self.stream.next_input();
                                let byte = self.base + cursor + 1;
                                let next_position = if cursor + 1 == prepared.display().len() {
                                    next_object(parts, self.part + 1).map_or_else(
                                        || StreamingLayoutPosition::at(byte as u64),
                                        |(id, order)| StreamingLayoutPosition::with_gap(byte as u64, StreamingObjectGap::before(id, order)),
                                    )
                                } else { StreamingLayoutPosition::at(byte as u64) };
                                let admission = self.stream.session(window.text_system())?.finalize_logical_line(StreamingLineFinalization {
                                    input_id: self.stream.binding.input_id,
                                    segment_policy_id: self.stream.binding.segment_policy_id,
                                    ordinal, delimiter_range: Some(start..next_position), next_position,
                                })?;
                                self.stream.retain(admission)?;
                                cursor += 1;
                            }
                        }
                        self.fragment += 1;
                        continue;
                    }
                    self.spans.push((self.part, self.base, self.started..self.stream.admissions.len()));
                    self.base = self.base.checked_add(prepared.display().len()).ok_or(StreamingLayoutError::InvalidSegment)?;
                }
                ResidentPresentationRecordKind::ImageMarker(marker) => {
                    let (id, order) = object(&parts[self.part]).unwrap();
                    let (ordinal, leading) = self.stream.next_input();
                    let gap = next_object(parts, self.part + 1).map_or_else(
                        || StreamingObjectGap::after(id, order),
                        |(next_id, next_order)| StreamingObjectGap::between(id, order, next_id, next_order),
                    );
                    let trailing = StreamingLayoutPosition::with_gap(self.base as u64, gap);
                    let label: gpui::SharedString = format!("Image {}", marker.marker.label()).into();
                    let mut run = block.text.run(label.len());
                    run.font.weight = gpui::FontWeight::SEMIBOLD;
                    run.color = gpui::rgb(0x94a3b8).into();
                    let width = window.text_system().shape_line(label.clone(), px(12.0), &[run.clone()], None).width + px(12.0);
                    let admission = self.stream.session(window.text_system())?.admit_inline_object(StreamingInlineObject {
                        input_id: self.stream.binding.input_id,
                        segment_policy_id: self.stream.binding.segment_policy_id,
                        ordinal, id, order, leading, trailing,
                        presentation: gpui::SharedString::new_static(""), runs: Vec::new(), width,
                        height: px(20.0), baseline: px(15.0), background: None,
                    })?;
                    self.stream.retain(admission)?;
                    self.spans.push((self.part, self.base, self.started..self.stream.admissions.len()));
                }
                _ => return Err(StreamingLayoutError::InvalidSegment),
            }
            self.part += 1;
            self.fragment = 0;
            self.started = self.stream.admissions.len();
        }
        if *observations == 0 { return Ok(None); }
        *observations -= 1;
        let (ordinal, position) = self.stream.next_input();
        let terminal = self.stream.session(window.text_system())?.end_source(StreamingEndOfSource {
            input_id: self.stream.binding.input_id,
            segment_policy_id: self.stream.binding.segment_policy_id,
            ordinal, position, source_extent: self.base as u64,
        })?;
        self.stream.retain(terminal)?;
        let continuation = self.stream.continuation.unwrap();
        let height = continuation.block_offset.max(self.stream.binding.line_height);
        let charge = self.charge();
        let mut admissions = std::mem::take(&mut self.stream.admissions).into_iter().enumerate().peekable();
        let mut text = Vec::new();
        let mut markers = Vec::new();
        for (index, base, range) in &self.spans {
            let mut owned = Vec::new();
            while admissions.peek().is_some_and(|(index, _)| *index < range.end) {
                let (_, admission) = admissions.next().unwrap();
                for fragment in admission.fragments.iter() {
                    if let StreamingLayoutFragment::InlineObject(marker) = fragment {
                        markers.push((*index, marker.bounds));
                    }
                }
                owned.push(admission);
            }
            if matches!(parts[*index].kind, ResidentPresentationRecordKind::NarrativeReady { .. }) {
                text.push((*index, Arc::new(NarrativeLayout {
                    admissions: owned, valid: Cell::new(true), font_environment: continuation.font_environment,
                    height, width: self.stream.binding.wrap_width, line_height: self.stream.binding.line_height,
                    charge: 0, logical_base: *base,
                })));
            }
        }
        Ok(Some(InlinePartsLayout { text, markers, height, charge }))
    }
}
