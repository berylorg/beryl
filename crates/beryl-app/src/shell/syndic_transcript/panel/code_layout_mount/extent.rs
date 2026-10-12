use super::super::super::provider::IndexedSourceResponse;
use super::*;

pub(super) struct ResourceExtent {
    pub(super) index: CodeLayoutIndex,
    pub(super) complete: Option<f32>,
}

impl ResourceExtent {
    pub(super) fn supply(&mut self, page: &IndexedSourceResponse) -> Result<(), CodeLayoutError> {
        let Some(read) = self.index.next_read() else {
            return Ok(());
        };
        if read.range.start != page.range.start || read.range.end != page.range.end {
            return Ok(());
        }
        let mut leading = 0;
        if read.kind == CodeLayoutReadKind::Context {
            while page
                .bytes
                .get(leading)
                .is_some_and(|byte| byte & 0xc0 == 0x80)
            {
                leading += 1;
                if leading > 3 {
                    return Err(CodeLayoutError::InvalidPage);
                }
            }
        }
        let bytes = &page.bytes[leading..];
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(error)
                if error.error_len().is_none() && read.kind == CodeLayoutReadKind::Forward =>
            {
                std::str::from_utf8(&bytes[..error.valid_up_to()])
                    .map_err(|_| CodeLayoutError::InvalidPage)?
            }
            Err(_) => return Err(CodeLayoutError::InvalidPage),
        };
        if text.is_empty() {
            return Err(CodeLayoutError::InvalidPage);
        }
        self.index
            .supply_page(page.range.start + leading as u64, text)
    }
}

impl CodeLayoutMount {
    pub(super) fn advance_extent(
        &mut self,
        viewport: &CodeViewport,
        appearance: &CodeAppearance,
        window: &Window,
    ) -> Result<bool, CodeLayoutError> {
        self.query = None;
        let Some(total_lines) = self.total_lines else {
            return Ok(false);
        };
        if self.complete_width(total_lines).is_some() {
            return Ok(false);
        }
        if self.extent.is_none() {
            let source = self
                .binding
                .as_ref()
                .ok_or(CodeLayoutError::InvalidPage)?
                .range;
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(CodeLayoutError::Overflow)?;
            let binding = StreamingLayoutBinding {
                input_id: self.sequence,
                segment_policy_id: 1,
                start_position: StreamingLayoutPosition::at(source.start),
                wrap_width: px(1.0),
                font_size: px(appearance.body_font.size),
                line_height: px(viewport.scroll.borrow().line_height),
                limits: StreamingLayoutLimits {
                    segment_bytes: 1024,
                    runs: 8,
                    decorations: 8,
                    glyphs: 4096,
                    wraps: 1,
                    maps: 4097,
                    fragments: 1,
                    retained_items: 16384,
                    retained_bytes: 512 * 1024,
                },
            };
            let limits = CodeLayoutLimits {
                segment_bytes: 1024,
                page_bytes: 8192,
                segments_per_quantum: 1,
                boundary_steps_per_quantum: 2048,
                checkpoints: 32,
                fragments: 1,
                retained_bytes: 2 * 1024 * 1024,
                retained_items: 65536,
            };
            self.extent = Some(ResourceExtent {
                index: CodeLayoutIndex::new(binding, source.start..source.end, 0, limits)?,
                complete: None,
            });
        }
        let extent = self
            .extent
            .as_mut()
            .ok_or(CodeLayoutError::InvalidGeometry)?;
        if let Some(read) = extent.index.next_read() {
            self.query = Some(IndexedResourceDemand {
                revision: viewport.revision,
                root: self.root,
                query: IndexedResourceQuery::Source {
                    range: index::ByteRange {
                        start: read.range.start,
                        end: read.range.end,
                    },
                    bytes: 16384,
                },
            });
            return Ok(false);
        }
        let run = TextRun {
            len: 0,
            font: appearance.body_font.font.clone(),
            color: appearance.body_text.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        if extent.index.advance(window.text_system(), &run)? == CodeLayoutProgress::Complete {
            extent.complete = Some(f32::from(
                extent
                    .index
                    .complete_max_width()
                    .ok_or(CodeLayoutError::InvalidGeometry)?,
            ));
        }
        Ok(true)
    }
}
