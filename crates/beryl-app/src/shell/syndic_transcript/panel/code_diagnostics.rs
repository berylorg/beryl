use super::super::super::diagnostics::CodePanelDiagnostics;
use super::*;

impl SyndicTranscriptPanel {
    pub(in super::super) fn code_diagnostics(&self) -> Vec<CodePanelDiagnostics> {
        self.code_panels
            .iter()
            .filter(|panel| !panel.overridden)
            .map(|panel| {
                let pages = self
                    .host
                    .indexed_resource_pages(&panel.viewport.owner.resource_id);
                let response = pages.last();
                let mut source_bytes = panel.layout.source_bytes();
                let mut realized_source_range: Option<std::ops::Range<u64>> = None;
                let mut realized_visual_lines = 0;
                if let Some(response) = &response {
                    let binding = match &response.value {
                        IndexedResourceValue::Range(value) => Some(&value.binding),
                        IndexedResourceValue::Display(value) => Some(&value.binding),
                        IndexedResourceValue::Anchor(value) => Some(&value.binding),
                        IndexedResourceValue::Source(value) => Some(&value.binding),
                        IndexedResourceValue::Pending => None,
                    };
                    source_bytes = binding
                        .map(|binding| binding.range.end - binding.range.start)
                        .or(source_bytes);
                }
                let scroll = panel.viewport.scroll.borrow();
                let prepared: Vec<_> = panel.layout.prepared().collect();
                if !prepared.is_empty() {
                    realized_visual_lines += prepared.len();
                    for prepared in &prepared {
                        for fragment in &prepared.fragments {
                            match &mut realized_source_range {
                                Some(range) => {
                                    range.start = range.start.min(fragment.source.start);
                                    range.end = range.end.max(fragment.source.end);
                                }
                                None => realized_source_range = Some(fragment.source.clone()),
                            }
                        }
                    }
                }
                CodePanelDiagnostics {
                    key: panel.diagnostic_key,
                    source_continuity: panel.viewport.source_continuity,
                    source_bytes,
                    logical_lines: scroll.lines,
                    realized_source_range,
                    realized_visual_lines,
                    overscan: super::super::code_viewport::OVERSCAN,
                    wrap_mode: "no_wrap",
                    viewport_size: (
                        scroll.bounds.size.width.into(),
                        scroll.bounds.size.height.into(),
                    ),
                    selected_for_wheel: self.code_wheel_owner.get() == Some(panel.id),
                    anchor_present: panel.viewport.anchor.is_some(),
                    anchor_remap_result: panel.viewport.anchor_remap_result,
                    anchor_visual_offset: panel.viewport.anchor_visual_offset(),
                    reconciliation_micros: panel.reconciliation_micros,
                    layout_epoch: panel.layout.epoch,
                    layout_state: panel.layout.state(&panel.viewport),
                    layout_checkpoints: panel.layout.checkpoints(),
                    layout_fragments: prepared
                        .iter()
                        .map(|prepared| prepared.fragments.len())
                        .sum(),
                    layout_charged_bytes: panel
                        .layout
                        .charge()
                        .saturating_add(panel.actions.charge()),
                    complete_inline_extent: panel.layout.complete_width(scroll.lines),
                }
            })
            .collect()
    }
}
