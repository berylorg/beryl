use gpui::{ImageResourcePin, Window};

use super::*;

pub(crate) struct ImageMediaWindow {
    pub(super) inner: Arc<WindowState>,
}
pub(crate) struct ImageMediaInterest {
    pub(super) inner: Arc<InterestState>,
}

impl ImageMediaAttachment {
    pub(crate) fn new_window(
        &self,
        window: &mut Window,
    ) -> Result<ImageMediaWindow, ImageMediaError> {
        let core = self
            .inner
            .core
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?;
        let mut state = lock(&core.state);
        if state.stopping || self.inner.retired.load(Ordering::Acquire) {
            return Err(ImageMediaError(ImageMediaFailure::Retired));
        }
        state.windows.retain(|_, window| window.strong_count() != 0);
        let native_window = window.window_handle().window_id();
        if state
            .windows
            .values()
            .filter_map(Weak::upgrade)
            .any(|entry| entry.native_window == native_window)
        {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        if state.windows.len() >= beryl_state::MAX_RESTORABLE_WINDOWS {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let scope = window.image_pin_scope(&core.domain)?;
        let id = next_id(&mut state.next_window)?;
        let inner = Arc::new(WindowState {
            id,
            native_window,
            core: Arc::downgrade(&core),
            attachment: Arc::downgrade(&self.inner),
            scope,
            closed: AtomicBool::new(false),
            interests: Mutex::new(BTreeMap::new()),
        });
        state.windows.insert(id, Arc::downgrade(&inner));
        Ok(ImageMediaWindow { inner })
    }
}

impl ImageMediaWindow {
    pub(crate) fn attachment_id(&self) -> Option<u64> {
        self.inner
            .attachment
            .upgrade()
            .map(|attachment| attachment.id)
    }
    pub(crate) fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
    pub(crate) fn request(
        &self,
        demand: ImageSourceDemand,
    ) -> Result<ImageMediaInterest, ImageMediaError> {
        let core = self
            .inner
            .core
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?;
        let attachment = self
            .inner
            .attachment
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?;
        let mut state = lock(&core.state);
        if self.inner.closed.load(Ordering::Acquire)
            || attachment.retired.load(Ordering::Acquire)
            || state.stopping
        {
            return Err(ImageMediaError(ImageMediaFailure::Retired));
        }
        let config = core.limits.configuration();
        if core.native_progress.is_exhausted() {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let mut interests = lock(&self.inner.interests);
        interests.retain(|_, interest| interest.strong_count() != 0);
        if interests.len() >= config.window_pin_entries
            || state.queue.len() >= config.decoder_queue_items
        {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let id = next_id(&mut state.next_interest)?;
        let inner = Arc::new(InterestState {
            id,
            native_revision: core.native_progress.revision(),
            window: Arc::downgrade(&self.inner),
            core: Arc::downgrade(&core),
            cancellation: CommandCancellation::new(),
            status: Mutex::new(InterestStatus::Queued),
            event: Arc::new(signal::ChangeEvent::default()),
        });
        interests.insert(id, Arc::downgrade(&inner));
        state.queue.push_back(QueuedDemand {
            attachment: Arc::downgrade(&attachment),
            interest: Arc::downgrade(&inner),
            demand,
        });
        drop(interests);
        drop(state);
        core.work.notify_one();
        core.event.notify();
        Ok(ImageMediaInterest { inner })
    }

    pub(crate) fn prepare_ready(
        &self,
        interest: &ImageMediaInterest,
        window: &mut Window,
    ) -> Result<Option<ImageResourcePin>, ImageMediaError> {
        if !Weak::ptr_eq(&interest.inner.window, &Arc::downgrade(&self.inner))
            || self.inner.native_window != window.window_handle().window_id()
        {
            return Err(ImageMediaError(ImageMediaFailure::Stale));
        }
        if self.inner.closed.load(Ordering::Acquire) || interest.inner.cancellation.is_cancelled() {
            return Err(ImageMediaError(ImageMediaFailure::Cancelled));
        }
        let core = self
            .inner
            .core
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?;
        let mut state = lock(&core.state);
        let identity = match &*lock(&interest.inner.status) {
            InterestStatus::Ready(handle) => {
                return self.inner.scope.pin(handle).map(Some).map_err(Into::into);
            }
            InterestStatus::Failed(error) => return Err(ImageMediaError(*error)),
            InterestStatus::Prepared(identity) => *identity,
            _ => return Ok(None),
        };
        let flight = state
            .flights
            .get_mut(&identity)
            .ok_or(ImageMediaError(ImageMediaFailure::Stale))?;
        if flight.cancellation.is_cancelled() || !flight.interests.contains_key(&interest.inner.id)
        {
            return Err(ImageMediaError(ImageMediaFailure::Cancelled));
        }
        let worker = flight.worker;
        let phase = std::mem::replace(&mut flight.phase, FlightPhase::Publishing);
        let FlightPhase::Prepared(ticket) = phase else {
            flight.phase = phase;
            return Ok(None);
        };
        let result = window
            .upload_admitted_image(ticket)
            .map_err(ImageMediaFailure::Native);
        drop(state);
        core.finish_flight(identity, worker, result);
        if interest.inner.cancellation.is_cancelled() {
            return Err(ImageMediaError(ImageMediaFailure::Cancelled));
        }
        match &*lock(&interest.inner.status) {
            InterestStatus::Ready(handle) => {
                self.inner.scope.pin(handle).map(Some).map_err(Into::into)
            }
            InterestStatus::Failed(error) => Err(ImageMediaError(*error)),
            _ => Ok(None),
        }
    }

    pub(crate) fn close(&self) -> ImageResourceDrain {
        close_window(&self.inner)
    }
}

impl Drop for ImageMediaWindow {
    fn drop(&mut self) {
        self.close();
    }
}

impl ImageMediaInterest {
    #[cfg(test)]
    pub(crate) fn read_pixel_for_test(&self) -> Result<[u8; 4], ImageMediaError> {
        if !self.inner.is_live() {
            return Err(ImageMediaError(ImageMediaFailure::Cancelled));
        }
        let core = self
            .inner
            .core
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?;
        let handle = match &*lock(&self.inner.status) {
            InterestStatus::Ready(handle) => handle.clone(),
            InterestStatus::Failed(failure) => return Err(ImageMediaError(*failure)),
            _ => {
                return Err(ImageMediaError(ImageMediaFailure::Native(
                    ImageResourceError::Pending,
                )));
            }
        };
        core.domain.read_pixel_for_test(&handle).map_err(Into::into)
    }

    pub(crate) fn native_retry_allowed(&self, failure: ImageMediaFailure) -> bool {
        self.inner.is_live()
            && self
                .inner
                .core
                .upgrade()
                .is_some_and(|core| !core.native_progress.is_exhausted())
            && matches!(
                failure,
                ImageMediaFailure::Native(
                    ImageResourceError::Pending
                        | ImageResourceError::Capacity
                        | ImageResourceError::NativeFailure
                        | ImageResourceError::Retired
                )
            )
    }

    pub(crate) fn native_retry_ready(&self) -> bool {
        self.inner.is_live()
            && self
                .inner
                .core
                .upgrade()
                .is_some_and(|core| core.native_progress.revision() != self.inner.native_revision)
    }

    pub(crate) fn native_retry_changed(&self) -> ImageMediaChanged {
        let Some(core) = self.inner.core.upgrade() else {
            return self.inner.event.changed(true);
        };
        self.inner.event.native_retry_changed(
            core.native_progress.clone(),
            self.inner.native_revision,
            self.inner.cancellation.clone(),
        )
    }
    pub(crate) fn changed(&self) -> ImageMediaChanged {
        let pending = self.inner.event.changed(false);
        if self.inner.cancellation.is_cancelled()
            || matches!(
                *lock(&self.inner.status),
                InterestStatus::Prepared(_) | InterestStatus::Ready(_) | InterestStatus::Failed(_)
            )
        {
            self.inner.event.changed(true)
        } else {
            pending
        }
    }

    pub(crate) fn failure(&self) -> Option<ImageMediaFailure> {
        match *lock(&self.inner.status) {
            InterestStatus::Failed(error) => Some(error),
            _ if self.inner.cancellation.is_cancelled() => Some(ImageMediaFailure::Cancelled),
            _ => None,
        }
    }

    pub(crate) fn cancel(&self) {
        cancel_interest(&self.inner, ImageMediaFailure::Cancelled);
    }
}

impl Drop for ImageMediaInterest {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) fn cancel_interest(interest: &Arc<InterestState>, reason: ImageMediaFailure) {
    interest.cancellation.cancel();
    if let Some(core) = interest.core.upgrade() {
        let mut state = lock(&core.state);
        state.queue.retain(|queued| {
            !queued
                .interest
                .upgrade()
                .is_some_and(|entry| entry.id == interest.id)
        });
        let identity = match *lock(&interest.status) {
            InterestStatus::Flight(identity) | InterestStatus::Prepared(identity) => Some(identity),
            _ => None,
        };
        let mut remove = None;
        if let Some(identity) = identity {
            if let Some(flight) = state.flights.get_mut(&identity) {
                flight.interests.remove(&interest.id);
                flight
                    .interests
                    .retain(|_, interest| interest.strong_count() != 0);
                if flight.interests.is_empty() {
                    flight.cancellation.cancel();
                    if let Some(cancellation) = &flight.upload_cancellation {
                        cancellation.cancel();
                    }
                    if matches!(flight.phase, FlightPhase::Prepared(_)) {
                        remove = Some(identity);
                    }
                }
            }
        }
        if let Some(identity) = remove {
            state.flights.remove(&identity);
        }
        if let Some(window) = interest.window.upgrade() {
            lock(&window.interests).remove(&interest.id);
        }
        *lock(&interest.status) = InterestStatus::Failed(reason);
        state.cancellations = state.cancellations.saturating_add(1);
        drop(state);
        core.event.notify();
    }
    interest.event.notify();
}

pub(super) fn close_window(window: &Arc<WindowState>) -> ImageResourceDrain {
    let first = !window.closed.swap(true, Ordering::AcqRel);
    if first {
        let interests = lock(&window.interests)
            .values()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        for interest in interests {
            cancel_interest(&interest, ImageMediaFailure::Retired);
        }
        if let Some(core) = window.core.upgrade() {
            lock(&core.state).windows.remove(&window.id);
            core.event.notify();
        }
    }
    window.scope.close()
}
