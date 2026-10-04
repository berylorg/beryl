use super::{LifecycleAttentionRecord, LifecycleAttentionToken};
use crate::notice_limits::NOTICE_RECORD_CAPACITY;
use beryl_model::{SyndicThreadId, WindowId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleAttentionWindow {
    pub window_id: WindowId,
    pub viewed_thread: Option<SyndicThreadId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleAttentionRouteChange {
    Remove {
        token: LifecycleAttentionToken,
        window_id: WindowId,
    },
    Offer {
        record: LifecycleAttentionRecord,
        window_id: WindowId,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifecycleAttentionRouteDiagnostics {
    pub retained_routes: usize,
    pub destination_changes: u64,
    pub removals: u64,
    pub rejected_snapshots: u64,
}

#[derive(Clone)]
struct Route {
    token: LifecycleAttentionToken,
    window_id: WindowId,
}

pub struct LifecycleAttentionRouter {
    routes: Vec<Route>,
    diagnostics: LifecycleAttentionRouteDiagnostics,
}

impl Default for LifecycleAttentionRouter {
    fn default() -> Self {
        Self {
            routes: Vec::with_capacity(NOTICE_RECORD_CAPACITY),
            diagnostics: LifecycleAttentionRouteDiagnostics::default(),
        }
    }
}

impl LifecycleAttentionRouter {
    pub fn diagnostics(&self) -> LifecycleAttentionRouteDiagnostics {
        let mut diagnostics = self.diagnostics;
        diagnostics.retained_routes = self.routes.len();
        diagnostics
    }

    pub fn reconcile(
        &mut self,
        records: &[LifecycleAttentionRecord],
        windows: &[LifecycleAttentionWindow],
    ) -> Option<Vec<LifecycleAttentionRouteChange>> {
        if records.len() > NOTICE_RECORD_CAPACITY
            || windows.len() > beryl_state::MAX_RESTORABLE_WINDOWS
        {
            self.diagnostics.rejected_snapshots =
                self.diagnostics.rejected_snapshots.saturating_add(1);
            return None;
        }
        let destination = |thread: SyndicThreadId| {
            windows
                .iter()
                .filter(|window| window.viewed_thread == Some(thread))
                .map(|window| window.window_id)
                .min()
                .or_else(|| windows.iter().map(|window| window.window_id).min())
        };
        let mut changes = Vec::with_capacity(NOTICE_RECORD_CAPACITY * 2);
        self.routes.retain(|route| {
            let next = records
                .iter()
                .find(|record| record.token() == &route.token)
                .and_then(|record| destination(record.thread_id()));
            if next == Some(route.window_id) {
                return true;
            }
            if next.is_some() {
                self.diagnostics.destination_changes =
                    self.diagnostics.destination_changes.saturating_add(1);
            }
            self.diagnostics.removals = self.diagnostics.removals.saturating_add(1);
            changes.push(LifecycleAttentionRouteChange::Remove {
                token: route.token.clone(),
                window_id: route.window_id,
            });
            false
        });
        for record in records {
            let Some(window_id) = destination(record.thread_id()) else {
                continue;
            };
            if !self
                .routes
                .iter()
                .any(|route| &route.token == record.token())
            {
                self.routes.push(Route {
                    token: record.token().clone(),
                    window_id,
                });
            }
            changes.push(LifecycleAttentionRouteChange::Offer {
                record: record.clone(),
                window_id,
            });
        }
        Some(changes)
    }

    pub fn clear(&mut self) -> Vec<LifecycleAttentionRouteChange> {
        self.diagnostics.removals = self
            .diagnostics
            .removals
            .saturating_add(self.routes.len() as u64);
        self.routes
            .drain(..)
            .map(|route| LifecycleAttentionRouteChange::Remove {
                token: route.token,
                window_id: route.window_id,
            })
            .collect()
    }
}
