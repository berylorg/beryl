use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainWindowConversationComposerSurfaceSnapshot {
    pub selection: MainWindowComposerSelectionIdentity,
    pub binding: gpui_text_input::RangeBinding,
    pub publication: gpui_text_input::GeometryJobKey,
    pub source_selection: gpui_text_input::RangeSourceSelection,
    pub scroll_block: gpui::Pixels,
    pub viewport: gpui_text_input::ByteRange,
    pub overscan: gpui_text_input::ByteRange,
    pub quality: gpui_text_input::GeometryQuality,
    pub visual_lines: u64,
    pub content_height: gpui::Pixels,
    pub capacity: gpui_text_input::RangeRealizationCapacityState,
    pub fillers: [Option<gpui_text_input::RangeSurfaceFiller>; 2],
    pub realized_object_count: usize,
    pub realized_object_gap_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainWindowConversationComposerCompositeHit {
    pub selection: MainWindowComposerSelectionIdentity,
    pub binding: gpui_text_input::RangeBinding,
    pub publication: gpui_text_input::GeometryJobKey,
    pub hit: gpui_text_input::RangeSurfaceHit,
}

impl MainWindowConversationComposer {
    pub fn surface_snapshot(
        &self,
        cx: &App,
    ) -> Option<MainWindowConversationComposerSurfaceSnapshot> {
        self.input.read_with(cx, |input, _| {
            let surface = input.surface()?;
            let mut fillers = surface.fillers();
            Some(MainWindowConversationComposerSurfaceSnapshot {
                selection: self.selection,
                binding: surface.binding(),
                publication: surface.publication_key(),
                source_selection: surface.source_selection(),
                scroll_block: surface.scroll_block(),
                viewport: surface.viewport(),
                overscan: surface.overscan(),
                quality: surface.quality(),
                visual_lines: surface.visual_lines(),
                content_height: surface.content_height(),
                capacity: surface.capacity_state(),
                fillers: [fillers.next(), fillers.next()],
                realized_object_count: surface.realized_objects().len(),
                realized_object_gap_count: surface.realized_object_gaps().len(),
            })
        })
    }

    pub fn hit_test_composite_viewport(
        &self,
        viewport_position: gpui::Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<MainWindowConversationComposerCompositeHit> {
        self.input.read_with(cx, |input, _| {
            let surface = input.surface()?;
            let logical_position =
                viewport_position + gpui::point(gpui::Pixels::ZERO, surface.scroll_block());
            Some(MainWindowConversationComposerCompositeHit {
                selection: self.selection,
                binding: surface.binding(),
                publication: surface.publication_key(),
                hit: surface.hit_test_composite(logical_position)?,
            })
        })
    }

    pub fn request_absolute_scroll(
        &mut self,
        block_offset: gpui::Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.ensure_realization_control_available()?;
        self.input
            .update(cx, |input, input_cx| {
                input.request_absolute_scroll(block_offset, input_cx)
            })
            .map_err(|_| "composer absolute scroll was rejected".to_owned())?;
        self.schedule_pump(window, cx);
        Ok(())
    }

    pub fn request_filler_reanchor(
        &mut self,
        viewport_block: gpui::Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.ensure_realization_control_available()?;
        self.input
            .update(cx, |input, input_cx| {
                input.request_filler_reanchor(viewport_block, input_cx)
            })
            .map_err(|_| "composer filler reanchor was rejected".to_owned())?;
        self.schedule_pump(window, cx);
        Ok(())
    }

    fn ensure_realization_control_available(&self) -> Result<(), String> {
        if !self.is_live() {
            return Err("composer realization control is fenced".to_owned());
        }
        if self.last_error.is_some() {
            return Err("composer realization control is unavailable".to_owned());
        }
        Ok(())
    }
}
