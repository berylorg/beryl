use super::*;
use crate::runtime_setup_catalog::RuntimeSetupCatalog;

mod bootstrap;

const READ_CAPACITY: usize = 8;
const RESIDENT_ROWS: usize = PICKER_MAX_RESIDENT_PAGES * PICKER_PAGE_ROWS;

#[cfg(all(test, feature = "test-faults"))]
fn spawn_fixture_page_work<F, T>(
    executor: &gpui::BackgroundExecutor,
    work: crate::main_window::conversation_composer_mount::worker::ResourceWorker<F>,
) -> gpui::Task<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (sender, receiver) = futures_channel::oneshot::channel();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(work.run());
    });
    executor.spawn(async move {
        let output = receiver.await;
        worker.join().expect("fixture page worker join");
        output.expect("fixture page worker completion")
    })
}

fn activity_label(time: Option<beryl_state::UnixMillis>) -> String {
    let Some(time) = time else {
        return "No activity".into();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        });
    let minutes = now.saturating_sub(time.get()) / 60_000;
    match minutes {
        0 => "Just now".into(),
        1..=59 => format!("{minutes} min ago"),
        60..=1439 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1440),
    }
}

fn readiness_label(availability: beryl_model::Availability) -> &'static str {
    match availability {
        beryl_model::Availability::Available => "Ready",
        beryl_model::Availability::Unavailable(_) => "Unavailable",
        beryl_model::Availability::Unknown => "Not checked",
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn setup_read_current(
        &self,
        request: &PickerPageRequest,
        services: &PublishedRuntimeSetupServices,
        cancellation: &ProjectionCancellationToken,
        runtime: bool,
    ) -> bool {
        self.setup_enabled()
            && services.current()
            && !cancellation.is_cancelled()
            && self.runtime_setup.picker.is_some()
            && request.query_revision == self.runtime_setup.query_revision
            && request.collection_key
                == if runtime {
                    PickerCollectionKey("configured-runtimes".into())
                } else {
                    self.setup_collection_key()
                }
    }

    pub(in crate::main_window::shell::host) fn setup_catalog(
        &self,
    ) -> Option<(PublishedRuntimeSetupServices, RuntimeSetupCatalog)> {
        let services = self
            .runtime_setup
            .services
            .clone()
            .filter(|services| services.current())?;
        let catalog = RuntimeSetupCatalog {
            home: services.catalog_store(),
            state: services.state(),
        };
        Some((services, catalog))
    }

    pub(in crate::main_window::shell::host) fn request_setup_roots(
        &mut self,
        request: PickerPageRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.runtime_setup.picker.clone() else {
            return;
        };
        if !self.setup_enabled()
            || self.runtime_setup.workers.retained() + 2 > READ_CAPACITY
            || self
                .runtime_setup
                .page_jobs
                .iter()
                .any(|(pending, _, _)| pending == &request)
        {
            picker.update(cx, |picker, pcx| {
                picker.settle_page(PickerPageOutcome::Cancelled(request), window, pcx)
            });
            return;
        }
        let Some((services, catalog)) = self.setup_catalog() else {
            picker.update(cx, |picker, pcx| {
                picker.settle_page(PickerPageOutcome::Cancelled(request), window, pcx)
            });
            return;
        };
        let cancellation = ProjectionCancellationToken::new();
        let cancel = cancellation.clone();
        let revision = self.runtime_setup.revision;
        let scope = self.runtime_setup.scope;
        let query = self.runtime_setup.query.clone();
        let start = request.range.start;
        let count = request.range.len();
        let output_release = self.runtime_setup.workers.track(|| ());
        let observed_services = services.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let fail_root_page = self.runtime_setup.fixture_fail_root_page.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_delivery = self.runtime_setup.fixture_page_delivery.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_threaded = fixture_delivery.is_some();
        #[cfg(all(test, feature = "test-faults"))]
        let delivery_revision = request.query_revision;
        let work = self.runtime_setup.workers.track(move || {
            let result = (|| {
                #[cfg(all(test, feature = "test-faults"))]
                if fail_root_page
                    .is_some_and(|fail| fail.swap(false, std::sync::atomic::Ordering::AcqRel))
                {
                    return Err("Fixture root collection read failed.".to_owned());
                }
                let observation = observed_services.observe()?;
                let revision = revision.map_or_else(
                    || {
                        catalog
                            .home
                            .home_revision()
                            .map_err(|error| error.to_string())
                    },
                    Ok,
                )?;
                catalog
                    .root_page(revision, scope, &query, start, count, &cancel)
                    .map(|page| (page, observation))
            })();
            #[cfg(all(test, feature = "test-faults"))]
            if let Some(delivery) = fixture_delivery {
                delivery(false, delivery_revision);
            }
            (result, output_release)
        });
        #[cfg(all(test, feature = "test-faults"))]
        let job = if fixture_threaded {
            spawn_fixture_page_work(cx.background_executor(), work)
        } else {
            cx.background_executor().spawn(async move { work.run() })
        };
        #[cfg(not(all(test, feature = "test-faults")))]
        let job = cx.background_executor().spawn(async move { work.run() });
        let pending = request.clone();
        let settled_cancel = cancellation.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let (result, _release) = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup
                    .page_jobs
                    .retain(|(pending, _, _)| pending != &request);
                let Some(picker) = root.runtime_setup.picker.clone() else {
                    return;
                };
                if !root.setup_read_current(&request, &services, &settled_cancel, false) {
                    picker.update(cx, |picker, pcx| {
                        picker.settle_page(PickerPageOutcome::Cancelled(request), window, pcx)
                    });
                    return;
                }
                let outcome =
                    if !root.setup_read_current(&request, &services, &settled_cancel, false) {
                        PickerPageOutcome::Cancelled(request)
                    } else {
                        match result {
                            Ok((page, observation))
                                if root
                                    .runtime_setup
                                    .revision
                                    .is_none_or(|revision| revision == page.revision) =>
                            {
                                root.runtime_setup.revision = Some(page.revision);
                                if !root.runtime_setup.bootstrap_ready {
                                    root.runtime_setup.bootstrap_observation = Some(observation);
                                }
                                let rows = page
                                    .rows
                                    .iter()
                                    .map(|row| PickerRow {
                                        key: PickerRowKey(format!("root:{}", row.root.root_id())),
                                        primary: row.root.display_path().as_str().into(),
                                        secondary: format!(
                                            "{} threads - {}",
                                            row.thread_count,
                                            activity_label(row.root.last_activity_at())
                                        ),
                                        status: String::new(),
                                        tooltip: Some(row.root.display_path().as_str().into()),
                                        unavailable_reason: None,
                                        current: false,
                                        activation_pending: false,
                                    })
                                    .collect();
                                root.runtime_setup.roots.extend(page.rows);
                                while root.runtime_setup.roots.len() > RESIDENT_ROWS {
                                    root.runtime_setup.roots.pop_front();
                                }
                                PickerPageOutcome::Success(PickerPage {
                                    request,
                                    total_count: page.total,
                                    rows,
                                })
                            }
                            Ok(_) => PickerPageOutcome::Failed {
                                request,
                                message: "Runtime/root collection changed during read.".into(),
                            },
                            Err(message) => PickerPageOutcome::Failed { request, message },
                        }
                    };
                if !root.runtime_setup.bootstrap_ready {
                    if let PickerPageOutcome::Success(page) = outcome {
                        root.runtime_setup.bootstrap_root_page = Some(page);
                        root.publish_setup_bootstrap(window, cx);
                        return;
                    }
                    root.fail_setup_bootstrap(window, cx);
                }
                picker.update(cx, |picker, pcx| picker.settle_page(outcome, window, pcx));
                cx.notify();
            });
        });
        self.runtime_setup
            .page_jobs
            .push((pending, cancellation, task));
    }

    pub(in crate::main_window::shell::host) fn request_setup_runtimes(
        &mut self,
        request: PickerPageRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.runtime_setup.picker.clone() else {
            return;
        };
        if !self.setup_enabled()
            || self.runtime_setup.workers.retained() + 2 > READ_CAPACITY
            || self
                .runtime_setup
                .page_jobs
                .iter()
                .any(|(pending, _, _)| pending == &request)
        {
            picker.update(cx, |picker, pcx| {
                picker.settle_runtime_page(
                    PickerRuntimePageOutcome::Cancelled(request),
                    window,
                    pcx,
                )
            });
            return;
        }
        let Some((services, catalog)) = self.setup_catalog() else {
            picker.update(cx, |picker, pcx| {
                picker.settle_runtime_page(
                    PickerRuntimePageOutcome::Cancelled(request),
                    window,
                    pcx,
                )
            });
            return;
        };
        let cancellation = ProjectionCancellationToken::new();
        let cancel = cancellation.clone();
        let revision = self.runtime_setup.revision;
        let start = request.range.start;
        let count = request.range.len();
        let output_release = self.runtime_setup.workers.track(|| ());
        let observed_services = services.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_delivery = self.runtime_setup.fixture_page_delivery.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_threaded = fixture_delivery.is_some();
        #[cfg(all(test, feature = "test-faults"))]
        let delivery_revision = request.query_revision;
        let work = self.runtime_setup.workers.track(move || {
            let result = (|| {
                let observation = observed_services.observe()?;
                let revision = revision.map_or_else(
                    || {
                        catalog
                            .home
                            .home_revision()
                            .map_err(|error| error.to_string())
                    },
                    Ok,
                )?;
                catalog
                    .runtime_page(revision, start, count, &cancel)
                    .map(|page| (page, observation))
            })();
            #[cfg(all(test, feature = "test-faults"))]
            if let Some(delivery) = fixture_delivery {
                delivery(true, delivery_revision);
            }
            (result, output_release)
        });
        #[cfg(all(test, feature = "test-faults"))]
        let job = if fixture_threaded {
            spawn_fixture_page_work(cx.background_executor(), work)
        } else {
            cx.background_executor().spawn(async move { work.run() })
        };
        #[cfg(not(all(test, feature = "test-faults")))]
        let job = cx.background_executor().spawn(async move { work.run() });
        let pending = request.clone();
        let settled_cancel = cancellation.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let (result, _release) = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup
                    .page_jobs
                    .retain(|(pending, _, _)| pending != &request);
                let Some(picker) = root.runtime_setup.picker.clone() else {
                    return;
                };
                if !root.setup_read_current(&request, &services, &settled_cancel, true) {
                    picker.update(cx, |picker, pcx| {
                        picker.settle_runtime_page(
                            PickerRuntimePageOutcome::Cancelled(request),
                            window,
                            pcx,
                        )
                    });
                    return;
                }
                let outcome =
                    if !root.setup_read_current(&request, &services, &settled_cancel, true) {
                        PickerRuntimePageOutcome::Cancelled(request)
                    } else {
                        match result {
                            Ok((page, observation))
                                if root
                                    .runtime_setup
                                    .revision
                                    .is_none_or(|revision| revision == page.revision) =>
                            {
                                root.runtime_setup.revision = Some(page.revision);
                                if !root.runtime_setup.bootstrap_ready {
                                    root.runtime_setup.bootstrap_observation = Some(observation);
                                }
                                let rows = page
                                    .rows
                                    .iter()
                                    .map(|row| PickerRuntimeRow {
                                        row: PickerRow {
                                            key: PickerRowKey(format!(
                                                "runtime:{}",
                                                row.runtime.runtime_id()
                                            )),
                                            primary: row.runtime.environment_label().into(),
                                            secondary: format!(
                                                "{} · {} roots · {}",
                                                row.runtime.canonical_executable().as_str(),
                                                row.root_count,
                                                readiness_label(
                                                    row.runtime.availability().availability()
                                                )
                                            ),
                                            status: String::new(),
                                            tooltip: Some(
                                                row.runtime.canonical_executable().as_str().into(),
                                            ),
                                            unavailable_reason: None,
                                            current: false,
                                            activation_pending: false,
                                        },
                                        active_scope: root.runtime_setup.scope
                                            == Some(row.runtime.runtime_id()),
                                        browse_roots: if root.runtime_setup.scope
                                            == Some(row.runtime.runtime_id())
                                        {
                                            PickerCommandState::unavailable(
                                                "Roots shown",
                                                "This runtime's roots are already shown.",
                                            )
                                        } else {
                                            PickerCommandState::enabled("Browse roots")
                                        },
                                        add_root: PickerCommandState::enabled("Add root"),
                                    })
                                    .collect();
                                root.runtime_setup.runtimes.extend(page.rows);
                                while root.runtime_setup.runtimes.len() > RESIDENT_ROWS {
                                    root.runtime_setup.runtimes.pop_front();
                                }
                                PickerRuntimePageOutcome::Success(PickerRuntimePage {
                                    request,
                                    total_count: page.total,
                                    rows,
                                })
                            }
                            Ok(_) => PickerRuntimePageOutcome::Failed {
                                request,
                                message: "Runtime/root collection changed during read.".into(),
                            },
                            Err(message) => PickerRuntimePageOutcome::Failed { request, message },
                        }
                    };
                if !root.runtime_setup.bootstrap_ready {
                    if let PickerRuntimePageOutcome::Success(page) = outcome {
                        root.runtime_setup.bootstrap_runtime_page = Some(page);
                        root.publish_setup_bootstrap(window, cx);
                        return;
                    }
                    root.fail_setup_bootstrap(window, cx);
                }
                picker.update(cx, |picker, pcx| {
                    picker.settle_runtime_page(outcome, window, pcx)
                });
                cx.notify();
            });
        });
        self.runtime_setup
            .page_jobs
            .push((pending, cancellation, task));
    }

    pub(in crate::main_window::shell::host) fn refresh_setup_collections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_setup.cancel_reads();
        self.runtime_setup.revision = None;
        self.runtime_setup.bootstrap_root_page = None;
        self.runtime_setup.bootstrap_runtime_page = None;
        self.runtime_setup.bootstrap_observation = None;
        self.runtime_setup.bootstrap_ready = false;
        self.runtime_setup.roots.clear();
        self.runtime_setup.runtimes.clear();
        self.runtime_setup.query_revision = self
            .runtime_setup
            .query_revision
            .checked_add(1)
            .expect("setup query revision");
        let key = self.setup_collection_key();
        let revision = self.runtime_setup.query_revision;
        if let Some(picker) = self.runtime_setup.picker.clone() {
            picker.update(cx, |picker, pcx| {
                picker.replace_collection(key, revision, 0, None, window, pcx);
                picker.replace_runtime_collection(
                    PickerCollectionKey("configured-runtimes".into()),
                    revision,
                    0,
                    pcx,
                );
                picker.request_initial_page(pcx);
                picker.request_runtime_initial_page(pcx);
            });
        }
    }
}
