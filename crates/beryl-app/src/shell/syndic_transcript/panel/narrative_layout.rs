use std::{cell::Cell, mem::size_of, ops::Range, sync::Arc};

use gpui::{
    App, Bounds, Pixels, Point, StreamingEndOfSource, StreamingLayoutAdmission,
    StreamingLayoutBinding, StreamingLayoutContinuation, StreamingLayoutError,
    StreamingLayoutFragment, StreamingLayoutLimits, StreamingLayoutMap, StreamingLayoutPosition,
    StreamingLineFinalization, StreamingTextSegment, TextRun, Window, WindowTextSystem, point, px,
};
use syndic_storage::narrative::{
    NarrativeFragment, NarrativeRun, NarrativeStyle, PreparedNarrative,
};

pub(super) trait NarrativeDisplay {
    fn display(&self) -> &str;
    fn runs(&self) -> &[NarrativeRun];
    fn fragments(&self) -> &[NarrativeFragment];
}

impl NarrativeDisplay for PreparedNarrative {
    fn display(&self) -> &str {
        self.display()
    }
    fn runs(&self) -> &[NarrativeRun] {
        self.runs()
    }
    fn fragments(&self) -> &[NarrativeFragment] {
        self.fragments()
    }
}

pub(super) struct NarrativeLayoutPreparation {
    binding: StreamingLayoutBinding,
    continuation: Option<StreamingLayoutContinuation>,
    next_fragment: usize,
    admissions: Vec<StreamingLayoutAdmission>,
    charge: usize,
    allowance: usize,
    complete: bool,
}

pub(super) struct NarrativeLayout {
    admissions: Vec<StreamingLayoutAdmission>,
    valid: Cell<bool>,
    font_environment: Option<gpui::FontEnvironmentKey>,
    pub(super) height: Pixels,
    pub(super) width: Pixels,
    pub(super) line_height: Pixels,
    pub(super) charge: usize,
}

impl NarrativeLayoutPreparation {
    pub(super) fn new(
        input_id: u64,
        width: Pixels,
        font_size: Pixels,
        line_height: Pixels,
        allowance: usize,
    ) -> Self {
        Self {
            binding: StreamingLayoutBinding {
                input_id,
                segment_policy_id: 1,
                start_position: StreamingLayoutPosition::at(0),
                wrap_width: width,
                font_size,
                line_height,
                limits: StreamingLayoutLimits {
                    segment_bytes: 4096,
                    runs: 4096,
                    decorations: 4096,
                    glyphs: 4096,
                    wraps: 4096,
                    maps: 4097,
                    fragments: 1,
                    retained_items: 65_536,
                    retained_bytes: allowance,
                },
            },
            continuation: None,
            next_fragment: 0,
            admissions: Vec::new(),
            charge: size_of::<Self>(),
            allowance,
            complete: false,
        }
    }

    pub(super) fn charge(&self) -> usize {
        self.charge
    }

    pub(super) fn advance(
        &mut self,
        model: &impl NarrativeDisplay,
        text_system: &WindowTextSystem,
        observations: &mut usize,
        mut style: impl FnMut(NarrativeStyle, usize) -> TextRun,
    ) -> Result<Option<Arc<NarrativeLayout>>, StreamingLayoutError> {
        while self.next_fragment < model.fragments().len() {
            if *observations == 0 {
                return Ok(None);
            }
            *observations -= 1;
            let fragment = &model.fragments()[self.next_fragment];
            let mut cursor = fragment.display.start;
            while cursor < fragment.display.end {
                let next_break = model.display()[cursor..fragment.display.end]
                    .find('\n')
                    .map(|relative| cursor + relative);
                let end = next_break.unwrap_or(fragment.display.end);
                if cursor < end {
                    let runs = model.runs()[fragment.runs.clone()]
                        .iter()
                        .filter_map(|run| {
                            let start = run.display.start.max(cursor);
                            let stop = run.display.end.min(end);
                            (start < stop).then(|| style(run.style, stop - start))
                        })
                        .collect();
                    let (ordinal, start) = self.next_input();
                    let segment = StreamingTextSegment {
                        input_id: self.binding.input_id,
                        segment_policy_id: self.binding.segment_policy_id,
                        ordinal,
                        logical_range: start..StreamingLayoutPosition::at(end as u64),
                        text: model.display()[cursor..end].to_owned().into(),
                        runs,
                    };
                    let admission = self.session(text_system)?.admit_text(segment)?;
                    self.retain(admission)?;
                }
                cursor = end;
                if next_break.is_some() {
                    let (ordinal, start) = self.next_input();
                    let next_position = StreamingLayoutPosition::at((cursor + 1) as u64);
                    let admission = self.session(text_system)?.finalize_logical_line(
                        StreamingLineFinalization {
                            input_id: self.binding.input_id,
                            segment_policy_id: self.binding.segment_policy_id,
                            ordinal,
                            delimiter_range: Some(start..next_position),
                            next_position,
                        },
                    )?;
                    self.retain(admission)?;
                    cursor += 1;
                }
            }
            self.next_fragment += 1;
        }
        if !self.complete {
            let (ordinal, position) = self.next_input();
            let admission = self
                .session(text_system)?
                .end_source(StreamingEndOfSource {
                    input_id: self.binding.input_id,
                    segment_policy_id: self.binding.segment_policy_id,
                    ordinal,
                    position,
                    source_extent: model.display().len() as u64,
                })?;
            self.retain(admission)?;
            self.complete = true;
        }
        let height = self
            .continuation
            .unwrap()
            .block_offset
            .max(self.binding.line_height);
        Ok(Some(Arc::new(NarrativeLayout {
            admissions: std::mem::take(&mut self.admissions),
            valid: Cell::new(true),
            font_environment: self.continuation.unwrap().font_environment,
            height,
            width: self.binding.wrap_width,
            line_height: self.binding.line_height,
            charge: self.charge + size_of::<NarrativeLayout>() + 2 * size_of::<usize>(),
        })))
    }

    fn next_input(&self) -> (u64, StreamingLayoutPosition) {
        self.continuation
            .map_or((0, self.binding.start_position), |state| {
                (state.next_ordinal, state.next_position)
            })
    }

    fn session<'a>(
        &self,
        text_system: &'a WindowTextSystem,
    ) -> Result<gpui::StreamingLayoutSession<'a>, StreamingLayoutError> {
        match self.continuation {
            Some(state) => text_system.resume_streaming_layout_session(self.binding.clone(), state),
            None => text_system.streaming_layout_session(self.binding.clone()),
        }
    }

    fn retain(&mut self, admission: StreamingLayoutAdmission) -> Result<(), StreamingLayoutError> {
        let old_capacity = self.admissions.capacity();
        let next_capacity = if self.admissions.len() == self.admissions.capacity() {
            self.admissions.capacity().max(2).saturating_mul(2)
        } else {
            self.admissions.capacity()
        };
        let containers =
            next_capacity.saturating_sub(old_capacity) * size_of::<StreamingLayoutAdmission>();
        let bytes = admission.charge.total()?
            + containers
            + 2 * size_of::<usize>()
            + admission.fragments.len() * size_of::<StreamingLayoutFragment>();
        if self
            .charge
            .saturating_add(bytes)
            .saturating_add(size_of::<NarrativeLayout>() + 2 * size_of::<usize>())
            > self.allowance
        {
            return Err(StreamingLayoutError::CapacityExceeded(
                gpui::StreamingLayoutComponent::Total,
            ));
        }
        if next_capacity > self.admissions.capacity() {
            self.admissions
                .try_reserve_exact(next_capacity - self.admissions.len())
                .map_err(|_| {
                    StreamingLayoutError::CapacityExceeded(gpui::StreamingLayoutComponent::Total)
                })?;
        }
        let actual_containers = self.admissions.capacity().saturating_sub(old_capacity)
            * size_of::<StreamingLayoutAdmission>();
        let actual_bytes = bytes - containers + actual_containers;
        if self
            .charge
            .saturating_add(actual_bytes)
            .saturating_add(size_of::<NarrativeLayout>() + 2 * size_of::<usize>())
            > self.allowance
        {
            return Err(StreamingLayoutError::CapacityExceeded(
                gpui::StreamingLayoutComponent::Total,
            ));
        }
        self.charge += actual_bytes;
        self.continuation = Some(admission.continuation);
        self.admissions.push(admission);
        Ok(())
    }
}

fn maps(fragment: &StreamingLayoutFragment) -> &[StreamingLayoutMap] {
    match fragment {
        StreamingLayoutFragment::Text(value) => value.maps(),
        StreamingLayoutFragment::Boundary(value) => value.maps(),
        StreamingLayoutFragment::OversizeAtom(value) => value.maps(),
        StreamingLayoutFragment::InlineObject(value) => value.maps(),
    }
}

impl NarrativeLayout {
    pub(super) fn valid(&self) -> bool {
        self.valid.get()
    }

    pub(super) fn hit(&self, position: Point<Pixels>) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        let line_maps = || {
            self.admissions
                .iter()
                .flat_map(|admission| admission.fragments.iter())
                .flat_map(maps)
                .filter(|map| {
                    position.y >= map.position.y && position.y < map.position.y + self.line_height
                })
        };
        let mut line_left = None::<Pixels>;
        let mut line_right = None::<Pixels>;
        for map in line_maps() {
            line_left = Some(line_left.map_or(map.position.x, |left| left.min(map.position.x)));
            line_right = Some(line_right.map_or(map.position.x, |right| right.max(map.position.x)));
        }
        if position.x < line_left? || position.x > line_right? {
            return None;
        }
        line_maps()
            .min_by(|left, right| {
                let distance = |map: &StreamingLayoutMap| {
                    let y = f32::from(position.y - map.position.y);
                    let outside = if y < 0.0 {
                        -y
                    } else {
                        (y - f32::from(self.line_height)).max(0.0)
                    };
                    (outside, f32::from(position.x - map.position.x).abs())
                };
                let a = distance(left);
                let b = distance(right);
                a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
            })
            .map(|map| map.logical_position.byte_offset as usize)
    }

    pub(super) fn paint(
        &self,
        origin: Point<Pixels>,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), ()> {
        if !self.valid() || self.font_environment != window.text_system().font_environment_key() {
            self.valid.set(false);
            return Err(());
        }
        for admission in &self.admissions {
            for fragment in admission.fragments.iter() {
                if let Some(selected) = &selected {
                    let maps = maps(fragment);
                    for pair in maps.windows(2) {
                        let start = pair[0].logical_position.byte_offset as usize;
                        let end = pair[1].logical_position.byte_offset as usize;
                        if start >= selected.end || end <= selected.start {
                            continue;
                        }
                        let left = pair[0].position;
                        let right = pair[1].position;
                        let width = if left.y == right.y {
                            right.x - left.x
                        } else {
                            px(4.0)
                        };
                        if width > Pixels::ZERO {
                            window.paint_quad(gpui::fill(
                                Bounds::new(origin + left, gpui::size(width, self.line_height)),
                                gpui::rgba(0x38bdf840),
                            ));
                        }
                    }
                }
                if let StreamingLayoutFragment::Text(text) = fragment {
                    if text.paint(origin, window, cx).is_err() {
                        self.valid.set(false);
                        return Err(());
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn position(&self, index: usize) -> Option<Point<Pixels>> {
        self.admissions
            .iter()
            .flat_map(|value| value.fragments.iter())
            .flat_map(maps)
            .find(|map| map.logical_position.byte_offset == index as u64)
            .map(|map| map.position)
    }
}
