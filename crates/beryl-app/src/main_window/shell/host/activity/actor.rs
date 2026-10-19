use super::*;
use crate::activity_service::{
    ActivityHeightRead, ActivityPreparedFacts, ActivityPreparedPage, ActivityPreparedWindow,
    ActivityViewport, ActivityViewportStep, ActivityWindowSelection,
};
use std::time::Duration;

pub(super) enum Work {
    Step,
    HeightRead(Result<ActivityHeightRead, ActivityHeightError>),
    HeightCommit {
        value: f32,
        result: Result<Arc<crate::activity_service::ActivityHeightFlight>, ActivityHeightError>,
    },
}

pub(super) struct Frame {
    pub(super) work: Work,
    pub(super) step: ActivityViewportStep,
    pub(super) snapshot: Option<ActivityPreparedWindow>,
    pub(super) facts: Option<ActivityPreparedFacts>,
    pub(super) pages: Vec<Arc<model::Page>>,
    pub(super) retry: Option<ActivityRetry>,
    pub(super) failure: Option<Arc<str>>,
    pub(super) retrying: bool,
}

struct Control {
    demand: Option<ActivityDemand>,
    retry: Option<ActivityRetry>,
    height: Option<HeightCommand>,
}

type PageCache = Vec<(Arc<ActivityPreparedPage>, Arc<model::Page>)>;

impl Frame {
    fn capture(
        viewport: &ActivityViewport,
        step: ActivityViewportStep,
        work: Work,
        cache: &mut PageCache,
    ) -> Self {
        let snapshot = viewport.snapshot();
        let pages = snapshot.as_ref().map(|snapshot| snapshot.pages().iter().map(|page| {
            cache.iter().find(|(source, _)| Arc::ptr_eq(source, page)).map(|(_, page)| page.clone())
                .unwrap_or_else(|| Arc::new(model::Page {
                    start: page.start,
                    rows: page.rows.iter().map(|row| model::Row {
                        id: row.item_id(),
                        status: match row.status() {
                            syndic_storage::ActivityDisplayStatus::Running => model::Status::Running,
                            syndic_storage::ActivityDisplayStatus::FinishedOk => model::Status::FinishedOk,
                            syndic_storage::ActivityDisplayStatus::FinishedError => model::Status::FinishedError,
                        },
                        agent: row.agent().into(), value: row.value().into(), tooltip: row.tooltip().into(),
                    }).collect(),
                }))
        }).collect::<Vec<_>>()).unwrap_or_default();
        *cache = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .pages()
                    .iter()
                    .cloned()
                    .zip(pages.iter().cloned())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            work,
            step,
            snapshot,
            facts: viewport.prepared_facts(),
            pages,
            retry: viewport.retry_request(),
            failure: viewport.failure().map(Arc::from),
            retrying: viewport.retry_pending(),
        }
    }
}

impl MainWindowShellRoot {
    pub(super) fn start_activity(
        &mut self,
        selected: MainWindowComposerSelectionIdentity,
        reader: ActivityWindowReader,
        attempt: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lifetime = Arc::new(());
        let source = reader.select(
            selected.window_id(),
            selected.claim(),
            Arc::downgrade(&lifetime),
        );
        let demands = ActivityDemandOwner::new();
        let Some(demand) = demands.request(0..1) else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.activity.lifetime = Some(lifetime);
        self.activity.demands = Some(demands);
        self.activity.cancel = Some(cancel.clone());
        let native = window.window_handle();
        self.activity.task = Some(cx.spawn_in(window, async move |this, cx| {
            let mut viewport = ActivityViewport::new(source.clone(), demand);
            let mut cache = Vec::new();
            let mut read_height = true;
            let mut retained_height = None;
            let mut height_command = None;
            let mut metadata = super::metadata::Metadata::default();
            loop {
                let control = this
                    .update_in(cx, |root, window, cx| {
                        if cancel.load(Ordering::Acquire)
                            || window.window_handle() != native
                            || root.activity.attempt != attempt
                            || root.activity.selection != Some(selected)
                            || root
                                .cached_running_selection(cx)
                                .map(|(current, _)| current)
                                != Some(selected)
                            || root.shutdown_interaction_gated
                            || root.ordinary_close_interaction_gated
                            || root.startup_interaction_gated()
                        {
                            return None;
                        }
                        Some(Control {
                            demand: root.activity.demand.take(),
                            retry: root.activity.retry_command.take(),
                            height: root.activity.height_command.take(),
                        })
                    })
                    .ok()
                    .flatten();
                let Some(control) = control else {
                    break;
                };
                if let Some(height) = control.height {
                    height_command = Some(height);
                }
                let cancellation = cancel.clone();
                let selection = source.clone();
                let should_read_height = read_height;
                read_height = false;
                let executor = cx.background_executor().clone();
                let job = cx.background_executor().spawn(async move {
                    if let Some(demand) = control.demand {
                        viewport.set_demand(demand);
                    }
                    if let Some(retry) = control.retry {
                        viewport.retry_exact(&retry);
                    }
                    let (step, work) = if let Some(height) = retained_height.take() {
                        (ActivityViewportStep::Pending, Work::HeightRead(Ok(height)))
                    } else if should_read_height {
                        (
                            ActivityViewportStep::Pending,
                            Work::HeightRead(selection.read_height()),
                        )
                    } else if let Some(command) = height_command.take() {
                        let value = command.value;
                        let result = beryl_state::ActivityPanelHeight::new(value)
                            .map_err(|_| {
                                ActivityHeightError::State(
                                    beryl_state::SessionMutationError::InvalidCurrentState(
                                        "Activity height is not finite and positive",
                                    ),
                                )
                            })
                            .and_then(|height| {
                                selection.save_height_from_input(height, Some(&command.authority))
                            });
                        (
                            ActivityViewportStep::Pending,
                            Work::HeightCommit { value, result },
                        )
                    } else {
                        (viewport.step_visible(&cancellation), Work::Step)
                    };
                    metadata.step(&mut viewport, &executor);
                    let frame = Frame::capture(&viewport, step, work, &mut cache);
                    (viewport, cache, height_command, metadata, frame)
                });
                let (next, next_cache, pending_height, next_metadata, frame) = job.await;
                viewport = next;
                cache = next_cache;
                height_command = pending_height;
                metadata = next_metadata;
                let wait = !matches!(frame.step, ActivityViewportStep::Pending);
                let response = this
                    .update_in(cx, |root, window, cx| {
                        if cancel.load(Ordering::Acquire)
                            || root.activity.attempt != attempt
                            || root.activity.selection != Some(selected)
                            || window.window_handle() != native
                        {
                            return None;
                        }
                        Some(root.activity_frame(frame, window, cx))
                    })
                    .ok()
                    .flatten();
                let Some((height, reload, block_height)) = response else {
                    break;
                };
                retained_height = height;
                read_height |= reload;
                if block_height {
                    height_command = None;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(
                        if wait || retained_height.is_some() || reload {
                            100
                        } else {
                            1
                        },
                    ))
                    .await;
            }
        }));
    }
}
