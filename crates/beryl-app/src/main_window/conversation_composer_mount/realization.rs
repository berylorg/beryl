use super::*;

impl MainWindowConversationComposerMount {
    pub fn surface_snapshot(
        &self,
        cx: &App,
    ) -> Option<super::MainWindowConversationComposerSurfaceSnapshot> {
        self.contribution.as_ref().and_then(|contribution| {
            contribution.read_with(cx, |composer, composer_cx| {
                composer.surface_snapshot(composer_cx)
            })
        })
    }

    pub fn hit_test_composite_viewport(
        &self,
        viewport_position: gpui::Point<gpui::Pixels>,
        cx: &App,
    ) -> Option<super::MainWindowConversationComposerCompositeHit> {
        self.contribution.as_ref().and_then(|contribution| {
            contribution.read_with(cx, |composer, composer_cx| {
                composer.hit_test_composite_viewport(viewport_position, composer_cx)
            })
        })
    }

    pub fn request_absolute_scroll(
        &mut self,
        block_offset: gpui::Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer mount has no active contribution".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            composer.request_absolute_scroll(block_offset, window, composer_cx)
        })
    }

    pub fn request_filler_reanchor(
        &mut self,
        viewport_block: gpui::Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer mount has no active contribution".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            composer.request_filler_reanchor(viewport_block, window, composer_cx)
        })
    }
}
