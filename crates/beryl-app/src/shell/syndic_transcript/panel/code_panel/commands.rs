use super::*;

pub(super) fn clear_action_refusal(
    rejected: &mut Vec<crate::syndic_transcript::ResourceTranscriptDemand>,
    owner: &crate::syndic_transcript::ResourceTranscriptDemand,
    query: &crate::syndic_transcript::IndexedResourceDemand,
) -> bool {
    let previous = rejected.len();
    rejected.retain(|rejected| {
        !(super::source_actions::identity_matches(rejected, owner)
            && rejected.range == owner.range
            && rejected.indexed.as_ref() == Some(query))
    });
    rejected.len() != previous
}

pub(super) fn whole_copy_unavailable(
    code: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    pending: bool,
) -> Option<&'static str> {
    let owner = host.indexed_resource_owner(&code.viewport.owner.resource_id);
    if let Some(reason) =
        code.actions
            .whole_copy_unavailable(code.viewport.remapping, pending, owner.is_some())
    {
        return Some(reason);
    }
    let (Some(owner), Some((_, budget))) = (owner, code.actions.copy_budget(false)) else {
        return Some("Waiting for the current Code source binding");
    };
    if !host.can_reserve_code_layout(
        &owner,
        code.layout
            .charge()
            .saturating_add(code.actions.charge())
            .saturating_add(budget),
    ) {
        return Some("Code Copy exceeds available resource capacity");
    }
    None
}

impl SyndicTranscriptPanel {
    pub(crate) fn reject_code_source_candidate(
        &mut self,
        demand: &crate::syndic_transcript::ResourceTranscriptDemand,
    ) -> bool {
        if !self.resource_candidate_current(demand)
            || !self.code_panels.iter().any(|panel| {
                !panel.overridden
                    && panel.viewport.owner.resource_id == demand.resource_id
                    && panel.actions.query().as_ref() == demand.indexed.as_ref()
            })
        {
            return false;
        }
        self.discard_staged_activation();
        if !self.reject_code_source_action(demand) {
            return false;
        }
        self.reject_resource_demand(demand.clone());
        true
    }
    pub(crate) fn reject_code_source_action(
        &mut self,
        demand: &crate::syndic_transcript::ResourceTranscriptDemand,
    ) -> bool {
        if self.staged.is_some() || self.resource_demand().as_ref() != Some(demand) {
            return false;
        }
        let Some(panel) = self.code_panels.iter_mut().find(|panel| {
            !panel.overridden && panel.viewport.owner.resource_id == demand.resource_id
        }) else {
            return false;
        };
        let Some(owner) = self.host.indexed_resource_owner(&demand.resource_id) else {
            return false;
        };
        if owner.indexed != demand.indexed || !panel.actions.refuse_query(&owner) {
            return false;
        }
        self.host.reserve_code_layout(
            &owner,
            panel.layout.charge().saturating_add(panel.actions.charge()),
        );
        true
    }
    pub(in super::super) fn begin_code_copy(
        &mut self,
        id: u64,
        selected: bool,
        cx: &mut Context<Self>,
    ) {
        self.supersede_resource_copies();
        if self.staged.is_some() {
            if let Some(panel) = self.code_panels.iter_mut().find(|panel| panel.id == id) {
                panel.actions.feedback = "Copy unavailable: presentation changing";
                cx.notify();
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
        if panel.actions.copy.is_some() || panel.actions.save.is_some() {
            return;
        }
        if !selected && let Some(reason) = whole_copy_unavailable(panel, &self.host, false) {
            panel.actions.feedback = reason;
            cx.notify();
            return;
        }
        panel.actions.cancel_copy("");
        let Some(owner) = self
            .host
            .indexed_resource_owner(&panel.viewport.owner.resource_id)
        else {
            panel.actions.feedback = "Copy unavailable: current source owner is unavailable";
            cx.notify();
            return;
        };
        panel.actions.observe(
            &owner,
            panel.viewport.revision,
            self.host
                .indexed_resource_response(&owner.resource_id)
                .as_deref(),
        );
        let Some((range, budget)) = panel.actions.copy_budget(selected) else {
            panel.actions.feedback = "Copy unavailable: empty, unresolved or over 256 KiB";
            self.host.reserve_code_layout(
                &owner,
                panel.layout.charge().saturating_add(panel.actions.charge()),
            );
            cx.notify();
            return;
        };
        let reservation = panel
            .layout
            .charge()
            .saturating_add(panel.actions.charge())
            .saturating_add(budget);
        if !self.host.reserve_code_layout(&owner, reservation) {
            panel.actions.feedback = "Copy unavailable: capacity";
            cx.notify();
            return;
        }
        if !panel.actions.begin_copy(&owner, range, budget) {
            panel.actions.feedback = "Copy unavailable: allocation";
            self.host.reserve_code_layout(
                &owner,
                panel.layout.charge().saturating_add(panel.actions.charge()),
            );
        } else if let Some(query) = panel.actions.query() {
            clear_action_refusal(&mut self.rejected_resource, &owner, &query);
            cx.emit(super::super::ResourceViewportDemandChanged);
        }
        cx.notify();
    }

    pub(super) fn advance_code_source_actions(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        if self.advance_code_save(id, cx) {
            return true;
        }
        let Some(panel) = self
            .code_panels
            .iter_mut()
            .find(|panel| panel.id == id && !panel.overridden)
        else {
            return false;
        };
        let Some(owner) = self
            .host
            .indexed_resource_owner(&panel.viewport.owner.resource_id)
        else {
            panel
                .actions
                .cancel_copy("Copy unavailable: retired source");
            return false;
        };
        let response = self.host.indexed_resource_response(&owner.resource_id);
        let previous = (panel.actions.pending(), panel.actions.feedback);
        panel
            .actions
            .observe(&owner, panel.viewport.revision, response.as_deref());
        panel.actions.accept_copy_page(&owner, response.as_deref());
        if previous != (panel.actions.pending(), panel.actions.feedback) {
            self.host.reserve_code_layout(
                &owner,
                panel.layout.charge().saturating_add(panel.actions.charge()),
            );
            cx.notify();
        }
        if panel.actions.ready_copy(&owner) {
            let copy = panel.actions.copy.take().unwrap();
            let result = String::from_utf8(copy.bytes)
                .map_err(|_| ())
                .and_then(|text| {
                    let item = gpui::ClipboardItem::new_string(text);
                    let limits = gpui::ClipboardLimits {
                        total_bytes: copy.budget,
                        text_bytes: copy.budget,
                        metadata_bytes: 1,
                        image_bytes: 1,
                    };
                    #[cfg(test)]
                    if let Some(writer) = &mut panel.actions.writer {
                        return writer(&item, limits, cx).map_err(|_| ());
                    }
                    cx.write_to_clipboard_checked(&item, limits).map_err(|_| ())
                });
            panel.actions.feedback = if result.is_ok() {
                "Copied"
            } else {
                "Copy unavailable: clipboard refused"
            };
            self.host.reserve_code_layout(
                &owner,
                panel.layout.charge().saturating_add(panel.actions.charge()),
            );
            cx.notify();
        }
        if panel.viewport.remapping {
            return false;
        }
        if let Some(query) = panel.actions.query() {
            if owner.indexed.as_ref() != Some(&query)
                && self.host.set_indexed_resource_demand(&owner, Some(query))
            {
                cx.emit(super::super::ResourceViewportDemandChanged);
                cx.notify();
            }
            true
        } else {
            false
        }
    }
}
