use super::*;
use crate::{
    app_services::{PublishedRuntimeSetupServices, runtime_setup::RuntimeSetupFlight},
    cas_projection::ProjectionCancellationToken,
    runtime_admission::RuntimeAdmissionOutcome,
    thread_root_picker::*,
};
use std::{collections::VecDeque, time::Duration};

mod commands;
mod pages;
mod render;
#[cfg(all(test, feature = "test-faults"))]
mod test_access;
pub(super) use render::{render_command, render_picker};

pub(super) struct RuntimeSetupContribution {
    pub(super) services: Option<PublishedRuntimeSetupServices>,
    pub(super) focus: gpui::FocusHandle,
    pub(super) primary_focus: gpui::FocusHandle,
    pub(super) picker: Option<Entity<ThreadRootPicker>>,
    pub(super) subscription: Option<gpui::Subscription>,
    pub(super) scope: Option<beryl_model::RuntimeId>,
    pub(super) query: beryl_state::CatalogNormalizedQuery,
    pub(super) query_revision: u64,
    pub(super) revision: Option<beryl_model::HomeRevision>,
    pub(super) roots: VecDeque<crate::runtime_setup_catalog::SetupRootRow>,
    pub(super) runtimes: VecDeque<crate::runtime_setup_catalog::SetupRuntimeRow>,
    pub(super) page_jobs: Vec<(
        PickerPageRequest,
        ProjectionCancellationToken,
        gpui::Task<()>,
    )>,
    pub(super) workers: crate::main_window::conversation_composer_mount::worker::WorkerLifetime,
    pub(super) flight: Option<Arc<RuntimeSetupFlight>>,
    pub(super) poll: Option<gpui::Task<()>>,
    pub(super) native_dialog: bool,
    pub(super) native_task: Option<gpui::Task<()>>,
    pub(super) command: Option<PickerCommand>,
    pub(super) unavailable: Option<String>,
    pub(super) failure_notice: Option<crate::main_window::NoticeRecordToken>,
    pub(super) suspended: bool,
    pub(super) retired: bool,
    pub(super) mount: Option<Entity<MainWindowConversationComposerMount>>,
    pub(super) selected_root: Option<crate::runtime_setup_catalog::SetupRootRow>,
    pub(super) admission_task: Option<gpui::Task<()>>,
    pub(super) first_preparing: bool,
    pub(super) first_revalidating: bool,
    pub(super) transcript: Option<super::runtime_setup_attachment::SetupTranscript>,
    pub(super) transcript_task: Option<gpui::Task<()>>,
    pub(super) transcript_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub(super) reconciliation_retry_at: Option<std::time::Instant>,
    pub(super) mount_release_task: Option<gpui::Task<()>>,
    pub(super) mount_release: Option<crate::main_window::MainWindowComposerWidgetRelease>,
    pub(super) mount_release_error: Option<String>,
    bootstrap_root_page: Option<PickerPage>,
    bootstrap_runtime_page: Option<PickerRuntimePage>,
    bootstrap_observation:
        Option<crate::app_services::runtime_setup::PublishedRuntimeSetupObservation>,
    bootstrap_ready: bool,
    refresh_command: bool,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_services: Option<PublishedRuntimeSetupServices>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_members: Vec<beryl_model::WindowId>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_native: bool,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_transcript_reader:
        Option<crate::app_services::PublishedRunningThreadsReader>,
    #[cfg(all(test, feature = "test-faults"))]
    fixture_path_prompt: Option<(
        gpui::PathPromptOptions,
        futures_channel::oneshot::Sender<Result<Option<Vec<std::path::PathBuf>>, String>>,
    )>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_page_delivery: Option<Arc<dyn Fn(bool, u64) + Send + Sync>>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_admission: Option<Arc<RuntimeSetupFlight>>,
    #[cfg(all(test, feature = "test-faults"))]
    pub(super) fixture_fail_root_page: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl RuntimeSetupContribution {
    pub(super) fn new(cx: &mut Context<MainWindowShellRoot>) -> Self {
        Self {
            services: None,
            focus: cx.focus_handle(),
            primary_focus: cx.focus_handle(),
            picker: None,
            subscription: None,
            scope: None,
            query: beryl_state::CatalogNormalizedQuery::new("").expect("empty setup query"),
            query_revision: 1,
            revision: None,
            roots: VecDeque::new(),
            runtimes: VecDeque::new(),
            page_jobs: Vec::new(),
            workers: Default::default(),
            flight: None,
            poll: None,
            native_dialog: false,
            native_task: None,
            command: None,
            unavailable: None,
            failure_notice: None,
            suspended: false,
            retired: false,
            mount: None,
            selected_root: None,
            admission_task: None,
            first_preparing: false,
            first_revalidating: false,
            transcript: None,
            transcript_task: None,
            transcript_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            reconciliation_retry_at: None,
            mount_release_task: None,
            mount_release: None,
            mount_release_error: None,
            bootstrap_root_page: None,
            bootstrap_runtime_page: None,
            bootstrap_observation: None,
            bootstrap_ready: false,
            refresh_command: false,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_services: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_members: Vec::new(),
            #[cfg(all(test, feature = "test-faults"))]
            fixture_native: false,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_transcript_reader: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_path_prompt: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_page_delivery: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_admission: None,
            #[cfg(all(test, feature = "test-faults"))]
            fixture_fail_root_page: None,
        }
    }

    pub(super) fn cancel_reads(&mut self) {
        for (_, cancellation, _) in &self.page_jobs {
            cancellation.cancel();
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.native_dialog
            || self.flight.is_some()
            || self.unavailable.is_some()
            || self.command.is_some()
    }
}

impl Drop for RuntimeSetupContribution {
    fn drop(&mut self) {
        self.cancel_reads();
        self.transcript_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(flight) = &self.flight {
            flight.cancellation().cancel();
        }
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn reset_recovered_runtime_setup(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.runtime_setup.mount.is_some() || self.runtime_setup.workers.retained() != 0 {
            return;
        }
        let focus = self.runtime_setup.focus.clone();
        let primary_focus = self.runtime_setup.primary_focus.clone();
        self.runtime_setup = RuntimeSetupContribution::new(cx);
        self.runtime_setup.focus = focus;
        self.runtime_setup.primary_focus = primary_focus;
    }

    pub(super) fn setup_enabled(&self) -> bool {
        self.controller.is_some()
            && !self.startup_interaction_gated()
            && !self.shutdown_interaction_gated
            && !self.ordinary_close_interaction_gated
            && self.runtime_setup.unavailable.is_none()
            && !self.runtime_setup.suspended
            && !self.runtime_setup.retired
            && self.running_threads.pending_activation.is_none()
    }

    pub(super) fn sync_runtime_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .runtime_setup
            .services
            .as_ref()
            .is_some_and(|services| !services.current())
        {
            self.runtime_setup.cancel_reads();
            if let Some(flight) = &self.runtime_setup.flight {
                flight.cancellation().cancel();
            }
            self.runtime_setup.picker = None;
            self.runtime_setup.subscription = None;
            self.runtime_setup.services = None;
        }
        if !self.setup_enabled() {
            if let Some(picker) = &self.runtime_setup.picker {
                picker.update(cx, |picker, cx| {
                    picker.set_pending_page_retry_allowed(false, cx)
                });
            }
            self.runtime_setup.cancel_reads();
            if let Some(flight) = &self.runtime_setup.flight {
                flight.cancellation().cancel();
            }
            return;
        }
        if self.runtime_setup.services.is_none() {
            #[cfg(all(test, feature = "test-faults"))]
            {
                self.runtime_setup.services = self
                    .runtime_setup
                    .fixture_services
                    .clone()
                    .filter(|s| s.current());
            }
            #[cfg(target_os = "windows")]
            if self.runtime_setup.services.is_none() {
                self.runtime_setup.services =
                    crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                        .and_then(|owner| owner.upgrade())
                        .and_then(|owner| owner.borrow().runtime_setup_services());
            }
        }
        let _ = window;
    }

    pub(in crate::main_window::shell::host) fn setup_collection_key(&self) -> PickerCollectionKey {
        PickerCollectionKey(self.runtime_setup.scope.map_or_else(
            || "new-thread-all-roots".into(),
            |runtime| format!("new-thread-roots-{runtime}"),
        ))
    }

    pub(in crate::main_window::shell::host) fn open_runtime_setup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.setup_enabled()
            || self.runtime_setup.pending()
            || self.runtime_setup.picker.is_some()
        {
            return;
        }
        let Some(services) = self
            .runtime_setup
            .services
            .clone()
            .filter(|services| services.current())
        else {
            return;
        };
        let _ = services;
        let appearance = self
            .controller
            .as_ref()
            .expect("checked controller")
            .appearance()
            .clone();
        self.runtime_setup.scope = None;
        self.runtime_setup.query =
            beryl_state::CatalogNormalizedQuery::new("").expect("empty query");
        self.runtime_setup.query_revision = self
            .runtime_setup
            .query_revision
            .checked_add(1)
            .expect("setup query revision");
        self.runtime_setup.revision = None;
        self.runtime_setup.roots.clear();
        self.runtime_setup.runtimes.clear();
        self.runtime_setup.selected_root = None;
        self.runtime_setup.bootstrap_root_page = None;
        self.runtime_setup.bootstrap_runtime_page = None;
        self.runtime_setup.bootstrap_ready = false;
        let config = ThreadRootPickerConfig {
            title: "New thread".into(),
            helper: "Choose a root before confirming a new thread.".into(),
            heading: "ROOTS FOR ALL RUNTIMES".into(),
            empty_text: "No roots are configured.".into(),
            search_placeholder: "Search roots".into(),
            owner_focus: self.runtime_setup.focus.clone(),
            appearance: Some(appearance.clone()),
            style: ThreadRootPickerStyle::default(),
            scrollbar_style: self
                .controller
                .as_ref()
                .expect("checked controller")
                .appearance
                .scrollbar
                .clone(),
        };
        let key = self.setup_collection_key();
        let revision = self.runtime_setup.query_revision;
        let picker = cx.new(|cx| ThreadRootPicker::new(config, key, revision, 0, window, cx));
        self.runtime_setup.subscription = Some(cx.subscribe_in(
            &picker,
            window,
            |root, _, event: &PickerEvent, window, cx| {
                root.setup_picker_event(event.clone(), window, cx)
            },
        ));
        self.runtime_setup.picker = Some(picker.clone());
        picker.update(cx, |picker, picker_cx| {
            picker.set_row_presentation(PickerRowPresentation::Root, picker_cx);
            picker.configure_selection(
                PickerSelectionMode::Confirmed {
                    confirm: PickerCommandState::unavailable(
                        "Confirm",
                        "Choose a root before confirming.",
                    ),
                },
                picker_cx,
            );
            picker.configure_runtime_section(
                PickerRuntimeSectionConfig {
                    heading: "RUNTIMES & ROOTS".into(),
                    empty_text: "No runtimes are configured.".into(),
                    add_runtime: PickerCommandState::enabled("Add runtime"),
                },
                PickerCollectionKey("configured-runtimes".into()),
                revision,
                0,
                window,
                picker_cx,
            );
            picker.request_initial_page(picker_cx);
            picker.request_runtime_initial_page(picker_cx);
            picker.set_retry_commands(
                Some(PickerCommandState::enabled("Retry")),
                Some(PickerCommandState::enabled("Retry")),
                picker_cx,
            );
            picker.focus_search(window, picker_cx);
        });
        cx.notify();
    }

    pub(in crate::main_window::shell::host) fn setup_picker_event(
        &mut self,
        event: PickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::RequestPage(request) => self.request_setup_roots(request, window, cx),
            PickerEvent::RequestRuntimePage(request) => {
                self.request_setup_runtimes(request, window, cx)
            }
            PickerEvent::QueryChanged {
                collection_key,
                query_revision,
                query,
            } => {
                if self.runtime_setup.pending()
                    || collection_key != self.setup_collection_key()
                    || query_revision <= self.runtime_setup.query_revision
                {
                    return;
                }
                match beryl_state::CatalogNormalizedQuery::new(query) {
                    Ok(query) => {
                        self.runtime_setup.cancel_reads();
                        self.runtime_setup.query = query;
                        self.runtime_setup.query_revision = query_revision;
                        self.runtime_setup.roots.clear();
                        self.runtime_setup.bootstrap_root_page = None;
                        self.runtime_setup.bootstrap_runtime_page = None;
                        self.runtime_setup.bootstrap_ready = false;
                        if let Some(picker) = &self.runtime_setup.picker {
                            picker.update(cx, |picker, pcx| {
                                picker.replace_runtime_collection(
                                    PickerCollectionKey("configured-runtimes".into()),
                                    query_revision,
                                    0,
                                    pcx,
                                );
                                picker.request_runtime_initial_page(pcx);
                            });
                        }
                    }
                    Err(error) => self.setup_failure(&error.to_string(), false),
                }
            }
            PickerEvent::Command(command) => self.setup_command(command, window, cx),
            PickerEvent::SelectionChanged(key) => {
                self.runtime_setup.selected_root = self
                    .runtime_setup
                    .roots
                    .iter()
                    .find(|row| key == PickerRowKey(format!("root:{}", row.root.root_id())))
                    .cloned();
                if let Some(picker) = &self.runtime_setup.picker {
                    picker.update(cx, |picker, cx| {
                        picker.configure_selection(
                            PickerSelectionMode::Confirmed {
                                confirm: PickerCommandState::unavailable(
                                    "Confirm",
                                    "Thread confirmation is not available yet.",
                                ),
                            },
                            cx,
                        )
                    });
                    picker.update(cx, |picker, cx| {
                        picker.set_selection_eligibility(
                            &key,
                            Some("Thread confirmation is not available yet.".into()),
                            cx,
                        )
                    });
                }
            }
            PickerEvent::Dismiss => {
                if self.runtime_setup.native_dialog {
                    return;
                }
                self.runtime_setup.cancel_reads();
                self.runtime_setup.picker = None;
                self.runtime_setup.subscription = None;
                if let Some(flight) = &self.runtime_setup.flight {
                    flight.cancellation().cancel();
                }
            }
            PickerEvent::Activate(_) => {}
        }
        cx.notify();
    }

    pub(super) fn setup_failure(&mut self, error: &str, terminal: bool) {
        let error: String = error.chars().take(1024).collect();
        let previous = self.runtime_setup.failure_notice.take();
        self.runtime_setup.failure_notice =
            self.publish_runtime_setup_failure(previous, &error, terminal);
        if terminal {
            self.runtime_setup.unavailable = Some(error);
        }
    }

    pub(in crate::main_window::shell) fn retire_runtime_setup(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(picker) = &self.runtime_setup.picker {
            picker.update(cx, |picker, cx| {
                picker.set_pending_page_retry_allowed(false, cx)
            });
        }
        self.runtime_setup.retired = true;
        self.runtime_setup
            .transcript_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        self.runtime_setup.cancel_reads();
        if let Some(flight) = &self.runtime_setup.flight {
            flight.cancellation().cancel();
        }
        self.runtime_setup.picker = None;
        self.runtime_setup.subscription = None;
        self.runtime_setup.services = None;
    }

    pub(in crate::main_window::shell::host) fn finish_setup_command(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.runtime_setup.command.take();
        self.runtime_setup.native_dialog = false;
        if let Some(picker) = &self.runtime_setup.picker {
            picker.update(cx, |picker, cx| {
                picker.set_native_dialog_open(false, cx);
                if let Some(command) = &command {
                    picker.finish_command(command, cx);
                    picker.restore_command_focus(command, window, cx);
                }
            });
        }
        cx.notify();
    }
}
