use std::{
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context as FutureContext, Poll, Waker},
    thread::{self, ThreadId},
};

use gpui::{App, AppContext, Context, Entity, Task};

use super::{
    AdapterRegistrationError, AppearanceGeneration, AppearancePublicationFailure,
    AppearancePublicationTarget, AppearanceWindowSetSnapshot, StalePublicationReason,
    WindowAdapterId, WindowEpochExhausted, WindowSetEpoch,
};

type PublicationResult = Result<(), AppearancePublicationFailure>;

pub trait PreparedWindowAppearance {
    fn validate(&self, cx: &App) -> Result<(), super::AdapterFailureClass>;

    fn commit(self: Box<Self>, cx: &mut App);
}

pub trait AppearanceWindowAdapter {
    fn id(&self) -> WindowAdapterId;

    fn prepare(
        &self,
        generation: Arc<AppearanceGeneration>,
        cx: &mut App,
    ) -> Result<Box<dyn PreparedWindowAppearance>, super::AdapterFailureClass>;
}

struct Reply {
    result: Mutex<Option<PublicationResult>>,
    ready: Condvar,
}

impl Reply {
    fn complete(&self, result: PublicationResult) {
        *self.result.lock().expect("appearance reply lock") = Some(result);
        self.ready.notify_one();
    }

    fn wait(&self) -> PublicationResult {
        let mut result = self.result.lock().expect("appearance reply lock");
        while result.is_none() {
            result = self.ready.wait(result).expect("appearance reply wait");
        }
        result.take().expect("completed appearance reply")
    }
}

struct Request {
    epoch: WindowSetEpoch,
    previous: Arc<AppearanceGeneration>,
    generation: Arc<AppearanceGeneration>,
    reply: Arc<Reply>,
}

struct Mailbox {
    snapshot: AppearanceWindowSetSnapshot,
    pending: Option<Request>,
    occupied: bool,
    receiver: Option<Waker>,
}

pub struct GpuiAppearancePublicationTarget {
    state: Arc<Mutex<Mailbox>>,
    gui_thread: ThreadId,
}

impl GpuiAppearancePublicationTarget {
    pub fn pending_publications(&self) -> usize {
        usize::from(self.state.lock().expect("appearance mailbox lock").occupied)
    }
}

impl AppearancePublicationTarget for GpuiAppearancePublicationTarget {
    fn snapshot(&self) -> AppearanceWindowSetSnapshot {
        self.state
            .lock()
            .expect("appearance mailbox lock")
            .snapshot
            .clone()
    }

    fn publish(
        &self,
        epoch: WindowSetEpoch,
        previous: Arc<AppearanceGeneration>,
        generation: Arc<AppearanceGeneration>,
    ) -> PublicationResult {
        if self.is_publication_thread() {
            return Err(AppearancePublicationFailure::Reentrant);
        }
        let reply = Arc::new(Reply {
            result: Mutex::new(None),
            ready: Condvar::new(),
        });
        let wake = {
            let mut state = self.state.lock().expect("appearance mailbox lock");
            if !state.snapshot.active {
                return Err(AppearancePublicationFailure::Unavailable);
            }
            if state.occupied {
                return Err(AppearancePublicationFailure::CapacityReached);
            }
            state.occupied = true;
            state.pending = Some(Request {
                epoch,
                previous,
                generation,
                reply: Arc::clone(&reply),
            });
            state.receiver.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
        reply.wait()
    }

    fn is_publication_thread(&self) -> bool {
        thread::current().id() == self.gui_thread
    }

    fn retire(&self) {
        let mut state = self.state.lock().expect("appearance mailbox lock");
        state.snapshot.active = false;
        if let Some(request) = state.pending.take() {
            request
                .reply
                .complete(Err(AppearancePublicationFailure::Unavailable));
            state.occupied = false;
        }
        if let Some(wake) = state.receiver.take() {
            wake.wake();
        }
    }
}

struct NextRequest(Arc<Mutex<Mailbox>>);

impl Future for NextRequest {
    type Output = Option<Request>;

    fn poll(self: Pin<&mut Self>, cx: &mut FutureContext<'_>) -> Poll<Self::Output> {
        let mut state = self.0.lock().expect("appearance mailbox lock");
        if let Some(request) = state.pending.take() {
            return Poll::Ready(Some(request));
        }
        if !state.snapshot.active {
            return Poll::Ready(None);
        }
        state.receiver = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub struct GpuiAppearanceWindowSet {
    target: Arc<GpuiAppearancePublicationTarget>,
    adapters: Vec<Box<dyn AppearanceWindowAdapter>>,
    _receiver: Task<()>,
}

impl GpuiAppearanceWindowSet {
    pub fn new(
        current: Arc<AppearanceGeneration>,
        capacity: NonZeroUsize,
        cx: &mut App,
    ) -> Entity<Self> {
        let target = Arc::new(GpuiAppearancePublicationTarget {
            state: Arc::new(Mutex::new(Mailbox {
                snapshot: AppearanceWindowSetSnapshot {
                    epoch: WindowSetEpoch::initial(),
                    count: 0,
                    capacity: capacity.get(),
                    current,
                    active: true,
                },
                pending: None,
                occupied: false,
                receiver: None,
            })),
            gui_thread: thread::current().id(),
        });
        cx.new(|cx: &mut Context<Self>| {
            let state = Arc::clone(&target.state);
            let task = cx.spawn(async move |owner, cx| {
                while let Some(request) = NextRequest(Arc::clone(&state)).await {
                    let reply = Arc::clone(&request.reply);
                    let result = owner
                        .update(cx, |owner, cx| owner.adopt(request, cx))
                        .unwrap_or(Err(AppearancePublicationFailure::Unavailable));
                    state.lock().expect("appearance mailbox lock").occupied = false;
                    reply.complete(result);
                }
                let _ = owner.update(cx, |owner, _| owner.retire());
            });
            Self {
                target,
                adapters: Vec::with_capacity(capacity.get()),
                _receiver: task,
            }
        })
    }

    pub fn target(&self) -> Arc<GpuiAppearancePublicationTarget> {
        Arc::clone(&self.target)
    }

    pub fn register(
        &mut self,
        adapter: Box<dyn AppearanceWindowAdapter>,
        cx: &mut Context<Self>,
    ) -> Result<(), AdapterRegistrationError> {
        let snapshot = self.target.snapshot();
        let id = adapter.id();
        if !snapshot.active {
            return Err(AdapterRegistrationError::Preparation {
                adapter: id,
                class: super::AdapterFailureClass::Unavailable,
            });
        }
        if self.adapters.len() == snapshot.capacity {
            return Err(AdapterRegistrationError::CapacityReached);
        }
        if self.adapters.iter().any(|existing| existing.id() == id) {
            return Err(AdapterRegistrationError::DuplicateIdentity(id));
        }
        let next_epoch = snapshot
            .epoch
            .checked_next()
            .map_err(|_| AdapterRegistrationError::WindowEpochExhausted)?;
        let prepared = adapter
            .prepare(Arc::clone(&snapshot.current), cx)
            .map_err(|class| AdapterRegistrationError::Preparation { adapter: id, class })?;
        prepared
            .validate(cx)
            .map_err(|class| AdapterRegistrationError::Preparation { adapter: id, class })?;
        {
            let state = self.target.state.lock().expect("appearance mailbox lock");
            if !state.snapshot.active
                || state.snapshot.epoch != snapshot.epoch
                || !Arc::ptr_eq(&state.snapshot.current, &snapshot.current)
            {
                return Err(AdapterRegistrationError::Preparation {
                    adapter: id,
                    class: super::AdapterFailureClass::Unavailable,
                });
            }
        }
        prepared.commit(cx);
        self.adapters.push(adapter);
        let mut state = self.target.state.lock().expect("appearance mailbox lock");
        let retired = !state.snapshot.active;
        state.snapshot.count = if retired { 0 } else { self.adapters.len() };
        state.snapshot.epoch = next_epoch;
        drop(state);
        if retired {
            self.adapters.clear();
        }
        cx.refresh_windows();
        Ok(())
    }

    pub fn unregister(&mut self, id: WindowAdapterId) -> Result<bool, WindowEpochExhausted> {
        let Some(index) = self.adapters.iter().position(|adapter| adapter.id() == id) else {
            return Ok(false);
        };
        let mut state = self.target.state.lock().expect("appearance mailbox lock");
        let next_epoch = state.snapshot.epoch.checked_next()?;
        let removed = self.adapters.remove(index);
        state.snapshot.count = self.adapters.len();
        state.snapshot.epoch = next_epoch;
        drop(state);
        drop(removed);
        Ok(true)
    }

    pub fn retire(&mut self) {
        self.target.retire();
        self.adapters.clear();
        self.target
            .state
            .lock()
            .expect("appearance mailbox lock")
            .snapshot
            .count = 0;
    }

    fn adopt(&mut self, request: Request, cx: &mut Context<Self>) -> PublicationResult {
        let snapshot = self.target.snapshot();
        if !snapshot.active {
            return Err(AppearancePublicationFailure::Unavailable);
        }
        if snapshot.epoch != request.epoch {
            return Err(AppearancePublicationFailure::Stale(
                StalePublicationReason::WindowSetEpoch,
            ));
        }
        if !Arc::ptr_eq(&snapshot.current, &request.previous) {
            return Err(AppearancePublicationFailure::Stale(
                StalePublicationReason::CurrentGeneration,
            ));
        }
        if request.generation.prepared().home() != snapshot.current.prepared().home() {
            return Err(AppearancePublicationFailure::Stale(
                StalePublicationReason::ForeignService,
            ));
        }
        let mut prepared = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            prepared.push((
                adapter.id(),
                adapter
                    .prepare(Arc::clone(&request.generation), cx)
                    .map_err(|class| AppearancePublicationFailure::Adapter {
                        adapter: adapter.id(),
                        class,
                    })?,
            ));
        }
        for (adapter, publication) in &prepared {
            publication
                .validate(cx)
                .map_err(|class| AppearancePublicationFailure::Adapter {
                    adapter: *adapter,
                    class,
                })?;
        }
        {
            let state = self.target.state.lock().expect("appearance mailbox lock");
            if !state.snapshot.active {
                return Err(AppearancePublicationFailure::Unavailable);
            }
            if state.snapshot.epoch != request.epoch {
                return Err(AppearancePublicationFailure::Stale(
                    StalePublicationReason::WindowSetEpoch,
                ));
            }
            if !Arc::ptr_eq(&state.snapshot.current, &request.previous) {
                return Err(AppearancePublicationFailure::Stale(
                    StalePublicationReason::CurrentGeneration,
                ));
            }
        }
        // The complete infallible adoption is admitted here; retirement cannot split it.
        for (_, publication) in prepared {
            publication.commit(cx);
        }
        let mut state = self.target.state.lock().expect("appearance mailbox lock");
        state.snapshot.current = request.generation;
        let retired = !state.snapshot.active;
        if retired {
            state.snapshot.count = 0;
        }
        drop(state);
        if retired {
            self.adapters.clear();
        }
        cx.refresh_windows();
        Ok(())
    }
}

impl Drop for GpuiAppearanceWindowSet {
    fn drop(&mut self) {
        self.retire();
    }
}
