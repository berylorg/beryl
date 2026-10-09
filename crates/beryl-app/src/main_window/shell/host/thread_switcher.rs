use super::*;
use crate::catalog_query::{
    PublishedCatalogCollection, PublishedCatalogQueryReader, PublishedCatalogQueryRequest,
    PublishedCatalogQueryResult,
};
use crate::thread_root_picker::*;
use beryl_home_store::CommandCancellation;
use beryl_model::{HomeRevision, RootId, RuntimeId, SyndicThreadId};
use beryl_state::{
    CatalogNormalizedQuery, CatalogQueryCriteria, CatalogQueryPageLimit, CatalogQueryScope,
};
use std::collections::VecDeque;

mod configuration;
mod events;
mod opening;
mod pages;
mod presentation;
mod render;
#[cfg(all(test, feature = "test-faults"))]
mod test_access;
pub(super) use render::{render_command, render_picker};

const READ_LIMIT: usize = 8;
const ROW_LIMIT: usize = PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS;

#[derive(Clone, Debug, Eq, PartialEq)]
enum SwitcherMode {
    All,
    Root {
        runtime: RuntimeId,
        root: RootId,
        path: String,
    },
    Roots(beryl_state::RuntimeRecord),
}
impl SwitcherMode {
    fn key(&self) -> PickerCollectionKey {
        PickerCollectionKey(match self {
            Self::All => "switcher-all".into(),
            Self::Root { runtime, root, .. } => format!("switcher-root-{runtime:?}-{root:?}"),
            Self::Roots(runtime) => format!("switcher-roots-{:?}", runtime.runtime_id()),
        })
    }
    fn scope(&self) -> CatalogQueryScope {
        match self {
            Self::Root { runtime, root, .. } => CatalogQueryScope::Root {
                runtime_id: *runtime,
                root_id: *root,
            },
            _ => CatalogQueryScope::All,
        }
    }
}

#[derive(Clone)]
struct ReadFence {
    opening: Arc<()>,
    picker: gpui::EntityId,
    revision: u64,
    mode: SwitcherMode,
}

pub(super) struct ThreadSwitcherContribution {
    pub(super) picker: Option<Entity<ThreadRootPicker>>,
    subscription: Option<gpui::Subscription>,
    focus: gpui::FocusHandle,
    reader: Option<PublishedCatalogQueryReader>,
    opening: Arc<()>,
    anchor: Option<PublishedCatalogCollection>,
    refined: Option<PublishedCatalogCollection>,
    mode: SwitcherMode,
    query: CatalogNormalizedQuery,
    revision: u64,
    home_revision: Option<HomeRevision>,
    count: usize,
    jobs: Vec<(u64, CommandCancellation, gpui::Task<()>)>,
    next_job: u64,
    opening_pending: bool,
    refinement_pending: bool,
    threads: VecDeque<(PickerRowKey, SyndicThreadId, Option<String>)>,
    roots: VecDeque<(PickerRowKey, beryl_state::CatalogRootRow)>,
    runtimes: VecDeque<(PickerRowKey, beryl_state::CatalogRuntimeRow)>,
    title: String,
    title_thread: Option<SyndicThreadId>,
    activation: Option<PickerRowKey>,
    command: Option<PickerCommand>,
    failure: Option<String>,
    retired: bool,
    refresh_proven: bool,
    primary_ready: bool,
    runtime_ready: bool,
    focus_pending: bool,
    disabled_reason: Option<String>,
    runtime_requests: Vec<PickerPageRequest>,
    page_requests: Vec<(bool, PickerPageRequest)>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_reader: Option<PublishedCatalogQueryReader>,
}
impl ThreadSwitcherContribution {
    pub(super) fn new(cx: &mut Context<MainWindowShellRoot>) -> Self {
        Self {
            picker: None,
            subscription: None,
            focus: cx.focus_handle().tab_stop(true),
            reader: None,
            opening: Arc::new(()),
            anchor: None,
            refined: None,
            mode: SwitcherMode::All,
            query: CatalogNormalizedQuery::new("").expect("empty query"),
            revision: 1,
            home_revision: None,
            count: 0,
            jobs: Vec::new(),
            next_job: 1,
            opening_pending: false,
            refinement_pending: false,
            threads: VecDeque::new(),
            roots: VecDeque::new(),
            runtimes: VecDeque::new(),
            title: "Loading selected thread…".into(),
            title_thread: None,
            activation: None,
            command: None,
            failure: None,
            retired: false,
            refresh_proven: false,
            primary_ready: false,
            runtime_ready: false,
            focus_pending: false,
            disabled_reason: None,
            runtime_requests: Vec::new(),
            page_requests: Vec::new(),
            #[cfg(all(test, feature = "test-faults"))]
            fixture_reader: None,
        }
    }
    fn cancel_jobs(&mut self) {
        for (_, cancellation, _) in &self.jobs {
            cancellation.cancel();
        }
        self.jobs.clear();
        self.page_requests.clear();
    }
    fn discard(&mut self) {
        self.cancel_jobs();
        self.refined = None;
        self.anchor = None;
        self.opening = Arc::new(());
        self.subscription = None;
        self.picker = None;
        self.threads.clear();
        self.roots.clear();
        self.runtimes.clear();
        self.runtime_requests.clear();
        self.activation = None;
        self.opening_pending = false;
        self.refinement_pending = false;
    }
    fn fence(&self) -> Option<ReadFence> {
        Some(ReadFence {
            opening: self.opening.clone(),
            picker: self.picker.as_ref()?.entity_id(),
            revision: self.revision,
            mode: self.mode.clone(),
        })
    }
    fn matches(&self, fence: &ReadFence) -> bool {
        !self.retired
            && Arc::ptr_eq(&self.opening, &fence.opening)
            && self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.entity_id() == fence.picker)
            && self.revision == fence.revision
            && self.mode == fence.mode
    }
    fn collection(&self) -> Option<&PublishedCatalogCollection> {
        self.refined.as_ref().or(self.anchor.as_ref())
    }
}
impl Drop for ThreadSwitcherContribution {
    fn drop(&mut self) {
        self.cancel_jobs();
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn focused_thread_switcher_owner(
        &self,
        window: &Window,
    ) -> Option<gpui::FocusHandle> {
        self.thread_switcher
            .focus
            .is_focused(window)
            .then(|| self.thread_switcher.focus.clone())
    }

    fn switcher_reader(&self, cx: &Context<Self>) -> Option<PublishedCatalogQueryReader> {
        #[cfg(all(test, feature = "test-faults"))]
        if let Some(reader) = &self.thread_switcher.fixture_reader {
            return Some(reader.clone());
        }
        crate::running_owner::RunningProcessOwner::mounted_owner(cx)
            .and_then(|owner| owner.upgrade())
            .and_then(|owner| owner.borrow().catalog_query_reader())
    }
    fn switcher_disabled_reason(&self, cx: &Context<Self>) -> Option<String> {
        if self.startup_interaction_gated() {
            return Some("Beryl is preparing its windows.".into());
        }
        if self.shutdown_interaction_gated || self.ordinary_close_interaction_gated {
            return Some("This window is waiting for its durable close state.".into());
        }
        if self.running_selection_interaction_gated(cx) || self.runtime_setup.pending() {
            return Some(
                "This window is waiting for its original selection or configuration request."
                    .into(),
            );
        }
        if self.thread_switcher.retired || self.controller.is_none() {
            return Some("The catalog service is unavailable.".into());
        }
        None
    }
    pub(super) fn sync_thread_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread_switcher.retired {
            return;
        }
        if self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
            || self.startup_interaction_gated()
        {
            self.close_thread_switcher(window, cx);
            return;
        }
        self.thread_switcher.reader = self.switcher_reader(cx);
        let title = self.coherent_selected_thread_title(cx);
        self.thread_switcher.title_thread =
            title.as_ref().map(|(selection, _)| selection.thread_id());
        self.thread_switcher.title = title
            .as_ref()
            .map(|(_, title)| title.text().unwrap_or("Untitled thread"))
            .unwrap_or("Selected thread")
            .into();
        self.thread_switcher.disabled_reason = self.switcher_disabled_reason(cx);
        if self
            .thread_switcher
            .anchor
            .as_ref()
            .is_some_and(|anchor| !anchor.is_current())
        {
            self.close_thread_switcher(window, cx);
        }
    }
    pub(in crate::main_window::shell) fn retire_thread_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_thread_switcher(window, cx);
        self.thread_switcher.reader = None;
        self.thread_switcher.retired = true;
    }
    pub(in crate::main_window::shell) fn reset_recovered_thread_switcher(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let focus = self.thread_switcher.focus.clone();
        self.thread_switcher = ThreadSwitcherContribution::new(cx);
        self.thread_switcher.focus = focus;
    }
    fn close_thread_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = self.thread_switcher.picker.clone();
        self.thread_switcher.discard();
        if let Some(picker) = picker {
            picker.update(cx, |picker, pcx| picker.dismiss(window, pcx));
        }
        cx.notify();
    }
    fn switcher_spawn(
        &mut self,
        request: PublishedCatalogQueryRequest,
        cancellation: CommandCancellation,
        window: &mut Window,
        cx: &mut Context<Self>,
        accept: impl FnOnce(
            &mut Self,
            Result<PublishedCatalogQueryResult, String>,
            &mut Window,
            &mut Context<Self>,
        ) + 'static,
    ) -> Result<(), String> {
        if self.thread_switcher.jobs.len() >= READ_LIMIT {
            return Err("The catalog is waiting for its pending pages.".into());
        }
        let fence = self
            .thread_switcher
            .fence()
            .ok_or("The picker is closed.")?;
        let identity = request.identity().clone();
        let id = self.thread_switcher.next_job;
        self.thread_switcher.next_job = id
            .checked_add(1)
            .ok_or("Catalog request identities are exhausted.")?;
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = request
                .receive()
                .await
                .map_err(|error| error.to_string())
                .and_then(|response| {
                    if response.qualifies(&identity) {
                        Ok(response.into_result())
                    } else {
                        Err("The catalog response is obsolete.".into())
                    }
                });
            let _ = this.update_in(cx, |root, window, cx| {
                root.thread_switcher
                    .jobs
                    .retain(|(pending, _, _)| *pending != id);
                if root.thread_switcher.matches(&fence) {
                    accept(root, result, window, cx);
                }
            });
        });
        self.thread_switcher.jobs.push((id, cancellation, task));
        Ok(())
    }
}
