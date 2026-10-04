use super::*;
use crate::{
    app_services::PublishedRunningThreadsReader,
    lifecycle_attention::{
        LifecycleAttentionRouteChange, LifecycleAttentionRouter, LifecycleAttentionToken,
        LifecycleAttentionWindow,
    },
    main_window::MainWindowNoticeIngress,
};
use beryl_model::WindowId;
use std::time::Duration;

pub(super) struct RunningThreadsAttentionRoutes {
    reader: Option<PublishedRunningThreadsReader>,
    router: LifecycleAttentionRouter,
    presentations: Vec<Presentation>,
}

struct Presentation {
    token: LifecycleAttentionToken,
    window_id: WindowId,
    ingress: MainWindowNoticeIngress,
}

impl Default for RunningThreadsAttentionRoutes {
    fn default() -> Self {
        Self {
            reader: None,
            router: LifecycleAttentionRouter::default(),
            presentations: Vec::with_capacity(crate::notice_limits::NOTICE_RECORD_CAPACITY),
        }
    }
}

struct Destination {
    facts: LifecycleAttentionWindow,
    window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
}

impl RunningThreadsAttentionRoutes {
    fn clear(&mut self, app: &mut App) {
        for presentation in self.presentations.drain(..) {
            presentation
                .ingress
                .remove_lifecycle_attention(&presentation.token, app);
        }
        self.router.clear();
        self.reader = None;
    }
}

impl RunningProcessOwner {
    pub(super) fn observe_lifecycle_attention(owner: &Rc<RefCell<Self>>, app: &mut App) {
        if owner.borrow().attention_task.is_some() {
            return;
        }
        Self::project_lifecycle_attention(owner, app);
        let weak = Rc::downgrade(owner);
        let task = app.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if !cx
                    .update(|app| {
                        let Some(owner) = weak.upgrade() else {
                            return false;
                        };
                        Self::project_lifecycle_attention(&owner, app);
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        owner.borrow_mut().attention_task = Some(task);
    }

    pub(crate) fn retire_lifecycle_attention_routing(&mut self, app: &mut App) {
        self.attention_task = None;
        self.attention_routes.clear(app);
    }

    fn project_lifecycle_attention(owner: &Rc<RefCell<Self>>, app: &mut App) {
        let mut owner = owner.borrow_mut();
        let reader = owner.running_threads_reader();
        let same = match (&owner.attention_routes.reader, &reader) {
            (Some(previous), Some(current)) => previous.same_publication(current),
            (None, None) => true,
            _ => false,
        };
        if !same {
            owner.attention_routes.clear(app);
        }
        let Some(reader) = reader else {
            return;
        };
        owner.attention_routes.reader = Some(reader.clone());
        let Some(records) = reader.attention_snapshot() else {
            return;
        };
        let destinations: Vec<_> = owner
            .process
            .windows
            .shells()
            .iter()
            .filter(|shell| shell.is_published())
            .filter_map(|shell| {
                let window = shell.window();
                let root = window.read(app).ok()?;
                let controller = root.controller()?;
                let viewed_thread = root.coherent_viewed_thread(app);
                Some(Destination {
                    facts: LifecycleAttentionWindow {
                        window_id: controller.window_id(),
                        viewed_thread,
                    },
                    window,
                })
            })
            .collect();
        let facts: Vec<_> = destinations
            .iter()
            .map(|destination| destination.facts)
            .collect();
        let Some(changes) = owner.attention_routes.router.reconcile(&records, &facts) else {
            return;
        };
        for change in changes {
            match change {
                LifecycleAttentionRouteChange::Remove { token, window_id } => {
                    if let Some(index) =
                        owner
                            .attention_routes
                            .presentations
                            .iter()
                            .position(|presentation| {
                                presentation.token == token && presentation.window_id == window_id
                            })
                    {
                        let presentation = owner.attention_routes.presentations.remove(index);
                        presentation.ingress.remove_lifecycle_attention(&token, app);
                    }
                }
                LifecycleAttentionRouteChange::Offer { record, window_id } => {
                    if !reader.current() {
                        owner.attention_routes.clear(app);
                        return;
                    }
                    let Some(destination) = destinations
                        .iter()
                        .find(|destination| destination.facts.window_id == window_id)
                    else {
                        continue;
                    };
                    let ingress = destination
                        .window
                        .update(app, |root, window, cx| root.notice_ingress(window, cx))
                        .ok();
                    let Some(ingress) = ingress else {
                        continue;
                    };
                    ingress.offer_lifecycle_attention(&record, &reader, app);
                    if let Some(previous) = owner
                        .attention_routes
                        .presentations
                        .iter_mut()
                        .find(|presentation| &presentation.token == record.token())
                    {
                        previous.ingress = ingress;
                    } else {
                        owner.attention_routes.presentations.push(Presentation {
                            token: record.token().clone(),
                            window_id,
                            ingress,
                        });
                    }
                }
            }
        }
    }
}
