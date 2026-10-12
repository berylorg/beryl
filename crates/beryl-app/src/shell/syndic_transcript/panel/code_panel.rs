use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use super::super::provider::resource_index as index;
use gpui::{
    AnyElement, AppContext, Context, MouseButton, SharedString, Window, canvas, div, point,
    prelude::*, px, size,
};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarScrollState, ScrollbarState, ScrollbarVisibilityPolicy, render_scrollbar,
};

use super::super::{
    IndexedResourceValue, RealizedFrameWindow, ResidentTranscriptSnapshot, ResourceKind,
    SyndicTranscriptHost,
};
use super::{
    SyndicTranscriptPanel,
    code_viewport::{CodeViewport, VIEWPORT_HEIGHT, shell},
};

static NEXT_PANEL: AtomicU64 = AtomicU64::new(1);

#[path = "code_diagnostics.rs"]
mod diagnostics;

#[path = "code_panel/commands.rs"]
mod commands;
#[path = "code_panel/copy_button.rs"]
mod copy_button;
#[path = "code_panel/save.rs"]
pub(super) mod save;
#[path = "code_panel/selection.rs"]
pub(super) mod selection;
#[path = "code_panel/source_actions.rs"]
pub(super) mod source_actions;

#[cfg(test)]
#[path = "code_panel/testing.rs"]
mod testing;

pub(super) struct MountedCodePanel {
    pub(super) viewport: CodeViewport,
    pub(super) id: u64,
    focus: gpui::FocusHandle,
    copy_focus: gpui::FocusHandle,
    save_focus: gpui::FocusHandle,
    owner: ScrollbarOwnerKey,
    scrollbar: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
    pub(super) overridden: bool,
    diagnostic_key: Option<[u8; 16]>,
    reconciliation_micros: u64,
    pub(super) layout: super::code_layout_mount::CodeLayoutMount,
    pub(super) actions: source_actions::CodeSourceActions,
    #[cfg(test)]
    painted_body_glyphs: Rc<Cell<usize>>,
    #[cfg(test)]
    painted_selection_quads: Rc<Cell<usize>>,
    #[cfg(test)]
    command_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    #[cfg(test)]
    save_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    #[cfg(test)]
    feedback_bounds: Rc<Cell<Option<(gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>)>>>,
}

impl MountedCodePanel {
    fn new(
        viewport: CodeViewport,
        wheel: Rc<Cell<Option<u64>>>,
        cx: &mut Context<SyndicTranscriptPanel>,
    ) -> Self {
        let id = NEXT_PANEL.fetch_add(1, Ordering::Relaxed);
        let mut diagnostic_key = [0; 16];
        let diagnostic_key = getrandom::fill(&mut diagnostic_key)
            .ok()
            .map(|_| diagnostic_key);
        let owner =
            ScrollbarOwnerKey::new(ScrollbarOwnerId::new(id), ScrollbarMountGeneration::new(1));
        let scrollbar = ScrollbarState::new(owner);
        let weak = cx.entity().downgrade();
        let visibility = scrollbar.managed(Rc::new(move |_, _, app| {
            let _ = weak.update(app, |_, cx| cx.notify());
        }));
        let scroll = viewport.scroll.clone();
        let set = scroll.clone();
        let page = scroll.clone();
        let acquire = wheel.clone();
        let weak = cx.entity().downgrade();
        let interaction = ScrollbarInteraction::new(
            move || {
                let scroll = scroll.borrow();
                Some(ScrollbarScrollState {
                    owner,
                    viewport_bounds: scroll.bounds,
                    content_size: size(
                        scroll.bounds.size.width,
                        px(scroll.lines as f32 * scroll.line_height),
                    ),
                    scroll_offset: point(px(scroll.horizontal), px(scroll.offset())),
                    page_distance: scroll.bounds.size,
                })
            },
            move |_, value| set.borrow_mut().set_offset(f32::from(value)),
            move |_, direction, distance| {
                let mut scroll = page.borrow_mut();
                let delta = if direction == gpui_scrollbar::ScrollDirection::Forward {
                    f32::from(distance)
                } else {
                    -f32::from(distance)
                };
                scroll.scroll_by(delta);
                wheel.set(Some(id));
            },
            move |_| acquire.set(Some(id)),
            |_| {},
            move |_, _, app| {
                let _ = weak.update(app, |_, cx| cx.notify());
            },
        );
        Self {
            viewport,
            id,
            focus: cx.focus_handle(),
            copy_focus: cx.focus_handle(),
            save_focus: cx.focus_handle(),
            owner,
            scrollbar,
            visibility,
            interaction,
            overridden: false,
            diagnostic_key,
            reconciliation_micros: 0,
            layout: Default::default(),
            actions: Default::default(),
            #[cfg(test)]
            painted_body_glyphs: Rc::default(),
            #[cfg(test)]
            painted_selection_quads: Rc::default(),
            #[cfg(test)]
            command_bounds: Rc::default(),
            #[cfg(test)]
            save_bounds: Rc::default(),
            #[cfg(test)]
            feedback_bounds: Rc::default(),
        }
    }

    pub(super) fn retire(&self, window: &mut Window, cx: &mut gpui::App) {
        self.scrollbar.unmount_viewport(self.owner, window, cx);
    }
}

impl SyndicTranscriptPanel {
    pub(super) fn reconcile_code_panels(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame: &RealizedFrameWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for panel in self.retired_code_panels.drain(..) {
            release_layout(&mut self.host, &panel.viewport.owner);
            if let Some(staged) = &mut self.staged {
                release_layout(&mut staged.host, &panel.viewport.owner);
            }
            panel.retire(window, cx);
        }
        self.code_panels.retain(|panel| {
            let retained = !panel.overridden
                && self.host.selected_view_id() == Some(&panel.viewport.owner.view_id)
                && frame
                    .records
                    .iter()
                    .any(|record| record.record_id == panel.viewport.owner.record_id)
                && snapshot.records.iter().any(|record| {
                    record.id == panel.viewport.owner.record_id
                        && matches!(
                            &record.kind,
                            super::super::ResidentPresentationRecordKind::ResourceReference {
                                resource_id,
                                resource_kind: ResourceKind::Code,
                                ..
                            } if resource_id == &panel.viewport.owner.resource_id
                                || saved_code_successor(&panel.viewport.owner, record)
                        )
                        && matches!(
                            &record.provenance.source,
                            super::super::ResidentRecordSource::Syndic(source)
                                if source.view_id == panel.viewport.owner.view_id
                        )
                });
            if !retained {
                release_layout(&mut self.host, &panel.viewport.owner);
                if let Some(staged) = &mut self.staged {
                    release_layout(&mut staged.host, &panel.viewport.owner);
                }
                panel.retire(window, cx);
                if self.code_wheel_owner.get() == Some(panel.id) {
                    self.code_wheel_owner.set(None);
                }
            }
            retained
        });
        if self.staged.is_some() {
            return;
        }
        for demand in self.host.code_panel_demands() {
            if demand.kind != ResourceKind::Code
                || demand.indexed.is_some()
                || !frame
                    .records
                    .iter()
                    .any(|record| record.record_id == demand.record_id)
            {
                continue;
            }
            let Some(metadata) = &demand.metadata else {
                continue;
            };
            let Ok(revision) = beryl_model::ProjectionRevision::new(metadata.revision.0) else {
                continue;
            };
            if let Some(panel) = self
                .code_panels
                .iter_mut()
                .find(|panel| panel.viewport.owner.record_id == demand.record_id)
            {
                panel.viewport.rebind(demand, revision);
                panel.layout.reset();
                if self.host.set_indexed_resource_demand(
                    &panel.viewport.owner,
                    Some(panel.viewport.query()),
                ) {
                    cx.notify();
                    cx.emit(super::ResourceViewportDemandChanged);
                }
                continue;
            }
            let viewport = CodeViewport::new(demand, revision);
            if self
                .host
                .set_indexed_resource_demand(&viewport.owner, Some(viewport.query()))
            {
                self.code_panels.push(MountedCodePanel::new(
                    viewport,
                    self.code_wheel_owner.clone(),
                    cx,
                ));
                self.code_panels.last_mut().unwrap().actions.save_admission =
                    self.save_admission.clone();
                cx.notify();
                cx.emit(super::ResourceViewportDemandChanged);
            }
        }
        let metrics = self
            .code_appearance
            .line_height(cx)
            .and_then(|(height, key)| {
                if window.text_system().font_environment_key() == Some(key) {
                    Ok((height, key))
                } else {
                    Err(super::code_layout::CodeLayoutError::FontEnvironmentChanged)
                }
            });
        for panel in &mut self.code_panels {
            let started = std::time::Instant::now();
            let viewport = &mut panel.viewport;
            let metric_result =
                metrics
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|&(height, key)| {
                        panel.layout.reconcile_environment(
                            viewport,
                            &self.code_appearance,
                            window,
                            height,
                            key,
                        )?;
                        viewport.set_line_height(height);
                        Ok(())
                    });
            if metric_result.is_err() {
                let _ = panel.layout.validate_environment(viewport, window);
            }
            if metric_result.is_ok() && panel.layout.take_range_restart() {
                let query = viewport.restart_range();
                if self
                    .host
                    .set_indexed_resource_demand(&viewport.owner, Some(query))
                {
                    cx.notify();
                    cx.emit(super::ResourceViewportDemandChanged);
                }
                continue;
            }
            let resource = viewport.owner.resource_id.clone();
            let Some(owner) = self.host.indexed_resource_owner(&resource) else {
                continue;
            };
            viewport.owner = owner;
            viewport.adopt_pages(&self.host.indexed_resource_pages(&resource));
            panel.actions.observe(
                &viewport.owner,
                viewport.revision,
                self.host.indexed_resource_response(&resource).as_deref(),
            );
            panel.layout.reconcile_range(viewport);
            if let Err(error) = metric_result {
                panel.layout.refused = Some(error);
                self.host.reserve_code_layout(
                    &viewport.owner,
                    panel.layout.charge().saturating_add(panel.actions.charge()),
                );
            }
            if viewport.scroll.borrow().overflow() == 0.0
                && self.code_wheel_owner.get() == Some(panel.id)
            {
                self.code_wheel_owner.set(None);
            }
            let query = if panel.actions.save.as_ref().is_some_and(|save| save.busy) {
                viewport
                    .owner
                    .indexed
                    .clone()
                    .unwrap_or_else(|| viewport.query())
            } else if viewport.remapping {
                viewport.query()
            } else {
                panel.actions.query().unwrap_or_else(|| viewport.query())
            };
            // Pending uses its admitted original query until construction makes progress.
            let pending = self
                .host
                .indexed_resource_response(&resource)
                .is_some_and(|response| matches!(response.value, IndexedResourceValue::Pending));
            if !pending
                && viewport
                    .owner
                    .indexed
                    .as_ref()
                    .is_some_and(|inner| inner.query != query.query)
            {
                if self
                    .host
                    .set_indexed_resource_demand(&viewport.owner, Some(query))
                {
                    cx.notify();
                    cx.emit(super::ResourceViewportDemandChanged);
                }
            }
            panel.reconciliation_micros =
                started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        }
        for panel in &mut self.code_panels {
            if panel.layout.queued || (panel.layout.refused.is_some() && !panel.actions.pending()) {
                continue;
            }
            panel.layout.queued = true;
            let id = panel.id;
            cx.defer_in(window, move |panel, window, cx| {
                panel.advance_code_layout(id, window, cx);
            });
        }
    }

    fn advance_code_layout(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.staged.is_none() && self.advance_code_source_actions(id, cx) {
            if let Some(panel) = self.code_panels.iter_mut().find(|panel| panel.id == id) {
                panel.layout.queued = false;
            }
            return;
        }
        let Some(panel) = self
            .code_panels
            .iter_mut()
            .find(|panel| panel.id == id && !panel.overridden)
        else {
            return;
        };
        panel.layout.queued = false;
        if self.staged.is_some() || panel.viewport.remapping {
            return;
        }
        let metrics = self
            .code_appearance
            .line_height(cx)
            .and_then(|(height, key)| {
                panel.layout.reconcile_environment(
                    &mut panel.viewport,
                    &self.code_appearance,
                    window,
                    height,
                    key,
                )?;
                panel.viewport.set_line_height(height);
                Ok(key)
            });
        panel.layout.reconcile_range(&mut panel.viewport);
        let resource = panel.viewport.owner.resource_id.clone();
        let Some(owner) = self.host.indexed_resource_owner(&resource) else {
            return;
        };
        let font_environment = match metrics {
            Ok(key) => key,
            Err(error) => {
                let _ = panel
                    .layout
                    .validate_environment(&mut panel.viewport, window);
                panel.layout.refused = Some(error);
                self.host.reserve_code_layout(
                    &owner,
                    panel.layout.charge().saturating_add(panel.actions.charge()),
                );
                cx.notify();
                return;
            }
        };
        if panel.layout.take_range_restart() {
            let query = panel.viewport.restart_range();
            if self.host.set_indexed_resource_demand(&owner, Some(query)) {
                cx.notify();
                cx.emit(super::ResourceViewportDemandChanged);
            }
            return;
        }
        let admission = panel
            .layout
            .charge()
            .saturating_add(panel.actions.charge())
            .saturating_add(1024 * 1024);
        if !self.host.reserve_code_layout(&owner, admission) {
            panel.layout.refused = Some(super::code_layout::CodeLayoutError::Capacity);
            cx.notify();
            return;
        }
        let pages = self.host.indexed_resource_pages(&resource);
        if let Err(error) = panel.layout.adopt(
            &panel.viewport,
            pages,
            &self.code_appearance,
            window,
            font_environment,
        ) {
            panel.layout.refused = Some(error);
            cx.notify();
            return;
        }
        for _ in 0..32 {
            if !self.host.reserve_code_layout(
                &owner,
                panel
                    .layout
                    .charge()
                    .saturating_add(panel.actions.charge())
                    .saturating_add(512 * 1024),
            ) {
                panel.layout.refused = Some(super::code_layout::CodeLayoutError::Capacity);
                cx.notify();
                break;
            }
            let result = panel
                .layout
                .advance(&mut panel.viewport, &self.code_appearance, window);
            let result = result.and_then(|more| {
                panel
                    .layout
                    .validate_environment(&mut panel.viewport, window)?;
                Ok(more)
            });
            if !self.host.reserve_code_layout(
                &owner,
                panel.layout.charge().saturating_add(panel.actions.charge()),
            ) {
                panel.layout.discard_candidate();
                panel.layout.refused = Some(super::code_layout::CodeLayoutError::Capacity);
                cx.notify();
                break;
            }
            match result {
                Ok(more) => {
                    if panel.layout.resolve_inline_anchor(&mut panel.viewport) {
                        cx.notify();
                    }
                    panel.layout.publish_candidate(&mut panel.viewport);
                    if let Some(query) = &panel.layout.query {
                        panel.viewport.layout_query = Some(query.clone());
                        if owner.indexed.as_ref() != Some(query)
                            && self
                                .host
                                .set_indexed_resource_demand(&owner, Some(query.clone()))
                        {
                            cx.emit(super::ResourceViewportDemandChanged);
                            cx.notify();
                        }
                    } else {
                        panel.viewport.layout_query = None;
                    }
                    let mut scroll = panel.viewport.scroll.borrow_mut();
                    if let Some(width) = panel.layout.complete_width(scroll.lines) {
                        let maximum = (width
                            - (f32::from(scroll.bounds.size.width) - 20.0).max(1.0))
                        .max(0.0);
                        scroll.horizontal = scroll.horizontal.min(maximum);
                    }
                    if more {
                        cx.notify();
                    } else {
                        break;
                    }
                }
                Err(error) => {
                    panel.layout.discard_candidate();
                    panel.layout.refused = Some(error);
                    cx.notify();
                    break;
                }
            }
        }
    }
}

pub(super) fn release_layout(
    host: &mut SyndicTranscriptHost,
    owner: &super::super::ResourceTranscriptDemand,
) {
    let Some(current) = host.indexed_resource_owner(&owner.resource_id) else {
        return;
    };
    if current.activation_revision == owner.activation_revision
        && current.view_id == owner.view_id
        && current.record_id == owner.record_id
        && current.revision == owner.revision
        && current.provenance == owner.provenance
    {
        host.reserve_code_layout(&current, 0);
    }
}

pub(super) fn saved_code_successor(
    owner: &super::super::ResourceTranscriptDemand,
    record: &super::super::ResidentPresentationRecord,
) -> bool {
    let super::super::ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        return false;
    };
    source.view_id == owner.provenance.view_id
        && source.item_id.is_some()
        && source.item_id == owner.provenance.item_id
        && source.projection_id.is_some()
        && source.projection_id != owner.provenance.projection_id
        && source
            .source_range
            .as_ref()
            .zip(owner.provenance.source_range.as_ref())
            .is_some_and(|(next, prior)| {
                next.end >= prior.end
                    && match (source.block_id, owner.provenance.block_id) {
                        (Some(next_block), Some(prior_block)) => {
                            next_block == prior_block
                                && (next.start == prior.start || prior.is_empty())
                        }
                        (None, None) => next.start == prior.start,
                        _ => false,
                    }
            })
}

#[path = "code_panel/render.rs"]
mod rendering;
pub(super) use rendering::render;
