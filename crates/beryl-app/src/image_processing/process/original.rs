use gpui::{ClipboardItem, ClipboardLimits, SavePathChoice};

use super::*;

mod capacity;
mod copy_effect;
mod preparation;
pub(crate) use capacity::OriginalCapacityChanged;
pub(super) use capacity::OriginalCapacityEvent;
pub(crate) use copy_effect::OriginalImageCopyEffect;

#[cfg(test)]
#[path = "../../../tests/unit/original_image_commands.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalImageRequest {
    Availability,
    Copy,
    Save,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalImageFailure {
    Cancelled,
    Stale,
    Unavailable,
    Unsupported,
    Capacity,
    Integrity,
    Worker,
    Destination,
    Write,
    Publish,
    Cleanup,
    Clipboard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OriginalImageAvailability {
    pub(crate) copy: Result<(), OriginalImageFailure>,
    pub(crate) save: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalImageState {
    Preparing,
    WaitingForSaveChoice,
    Available(OriginalImageAvailability),
    PreparedCopy,
    PreparedSave,
    Publishing,
    Settled(Result<(), OriginalImageFailure>),
}

pub(crate) struct OriginalImageCommand {
    inner: Arc<OriginalCommandState>,
}

pub(super) struct OriginalCommandState {
    core: Weak<ProcessCore>,
    window: Weak<WindowState>,
    attachment: Weak<AttachmentState>,
    request: OriginalImageRequest,
    demand: ImageSourceDemand,
    cancellation: CommandCancellation,
    data: Mutex<CommandData>,
    event: Arc<signal::ChangeEvent>,
}

struct CommandData {
    state: OriginalImageState,
    worker_active: bool,
    handle_alive: bool,
    delivered: bool,
    copy: Option<ClipboardItem>,
    encoded_bytes: usize,
    choice: Option<SavePathChoice>,
    picker_pending: bool,
    encoded_length: usize,
}

impl OriginalCommandState {
    pub(super) fn belongs_to(&self, attachment: &Arc<AttachmentState>) -> bool {
        Weak::ptr_eq(&self.attachment, &Arc::downgrade(attachment))
    }
    fn live(&self) -> bool {
        !self.cancellation.is_cancelled()
            && self
                .window
                .upgrade()
                .is_some_and(|window| !window.closed.load(Ordering::Acquire))
            && self
                .attachment
                .upgrade()
                .is_some_and(|attachment| !attachment.retired.load(Ordering::Acquire))
    }

    pub(super) fn encoded_bytes(&self) -> usize {
        lock(&self.data).encoded_bytes
    }
}

impl ImageMediaWindow {
    pub(crate) fn original_capacity_changed(
        &self,
    ) -> Result<OriginalCapacityChanged, ImageMediaError> {
        self.inner
            .core
            .upgrade()
            .ok_or(ImageMediaError(ImageMediaFailure::Retired))?
            .original_capacity
            .changed(Arc::downgrade(&self.inner))
    }

    pub(crate) fn request_original(
        &self,
        demand: ImageSourceDemand,
        request: OriginalImageRequest,
    ) -> Result<OriginalImageCommand, ImageMediaError> {
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
        if state.stopping || self.is_closed() || attachment.retired.load(Ordering::Acquire) {
            return Err(ImageMediaError(ImageMediaFailure::Retired));
        }
        if state.original.is_some() {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let inner = Arc::new(OriginalCommandState {
            core: Arc::downgrade(&core),
            window: Arc::downgrade(&self.inner),
            attachment: Arc::downgrade(&attachment),
            request,
            demand,
            cancellation: CommandCancellation::new(),
            data: Mutex::new(CommandData {
                state: if request == OriginalImageRequest::Save {
                    OriginalImageState::WaitingForSaveChoice
                } else {
                    OriginalImageState::Preparing
                },
                worker_active: false,
                handle_alive: true,
                delivered: false,
                copy: None,
                encoded_bytes: 0,
                choice: None,
                picker_pending: request == OriginalImageRequest::Save,
                encoded_length: 0,
            }),
            event: Arc::new(signal::ChangeEvent::default()),
        });
        state.original = Some(inner.clone());
        drop(state);
        core.work.notify_one();
        Ok(OriginalImageCommand { inner })
    }
}

impl OriginalImageCommand {
    pub(crate) fn state(&self) -> OriginalImageState {
        let Some(core) = self.inner.core.upgrade() else {
            return OriginalImageState::Settled(Err(OriginalImageFailure::Cancelled));
        };
        let mut state = lock(&core.state);
        let mut data = lock(&self.inner.data);
        let result = match data.state {
            OriginalImageState::Available(_) if !self.inner.live() => {
                OriginalImageState::Settled(Err(OriginalImageFailure::Stale))
            }
            result => result,
        };
        let released = if matches!(result, OriginalImageState::Settled(_)) {
            data.delivered = true;
            release_if_done(&mut state, &self.inner, &data)
        } else {
            false
        };
        drop(data);
        drop(state);
        core.event.notify();
        if released {
            core.original_capacity.notify();
        }
        result
    }

    pub(crate) fn changed(&self) -> ImageMediaChanged {
        let pending = self.inner.event.changed(false);
        if matches!(
            lock(&self.inner.data).state,
            OriginalImageState::Preparing | OriginalImageState::Publishing
        ) {
            pending
        } else {
            self.inner.event.changed(true)
        }
    }

    pub(crate) fn clipboard_limits(&self) -> ClipboardLimits {
        let config = self
            .inner
            .core
            .upgrade()
            .map(|core| core.limits.configuration())
            .unwrap_or_default();
        let data = lock(&self.inner.data);
        let capacity_slack = data.encoded_bytes.saturating_sub(data.encoded_length);
        let total = config
            .original_clipboard_total_bytes
            .saturating_sub(capacity_slack);
        ClipboardLimits {
            total_bytes: total,
            text_bytes: 1,
            metadata_bytes: config.original_clipboard_metadata_bytes,
            image_bytes: total,
        }
    }

    pub(crate) fn submit_save_choice(&self, choice: SavePathChoice) -> bool {
        let Some(core) = self.inner.core.upgrade() else {
            return false;
        };
        let mut state = lock(&core.state);
        let mut data = lock(&self.inner.data);
        if self.inner.request != OriginalImageRequest::Save || !data.picker_pending {
            return false;
        }
        data.picker_pending = false;
        if !self.inner.live() || data.state != OriginalImageState::WaitingForSaveChoice {
            let released = release_if_done(&mut state, &self.inner, &data);
            drop(data);
            drop(state);
            core.event.notify();
            if released {
                core.original_capacity.notify();
            }
            if let Some(attachment) = self.inner.attachment.upgrade() {
                attachment.event.notify();
            }
            return false;
        }
        data.choice = Some(choice);
        data.state = OriginalImageState::Preparing;
        drop(data);
        drop(state);
        core.work.notify_one();
        self.inner.event.notify();
        true
    }

    pub(crate) fn picker_finished(&self) {
        self.finish_picker(OriginalImageFailure::Cancelled);
    }

    pub(crate) fn fail_picker(&self, reason: OriginalImageFailure) {
        self.finish_picker(reason);
    }

    fn finish_picker(&self, reason: OriginalImageFailure) {
        if self.inner.request != OriginalImageRequest::Save {
            return;
        }
        if let Some(core) = self.inner.core.upgrade() {
            let mut state = lock(&core.state);
            let mut data = lock(&self.inner.data);
            if !data.picker_pending {
                return;
            }
            data.picker_pending = false;
            let was_live = self.inner.live();
            self.inner.cancellation.cancel();
            data.state = OriginalImageState::Settled(Err(reason));
            data.delivered = reason == OriginalImageFailure::Cancelled || !was_live;
            let released = release_if_done(&mut state, &self.inner, &data);
            drop(data);
            drop(state);
            self.inner.event.notify();
            if let Some(attachment) = self.inner.attachment.upgrade() {
                attachment.event.notify();
            }
            core.event.notify();
            if released {
                core.original_capacity.notify();
            }
        }
    }

    fn elect(&self, expected: OriginalImageState) -> bool {
        let Some(core) = self.inner.core.upgrade() else {
            return false;
        };
        let state = lock(&core.state);
        let mut data = lock(&self.inner.data);
        if state.stopping || !self.inner.live() || data.state != expected {
            return false;
        }
        data.state = OriginalImageState::Publishing;
        drop(data);
        drop(state);
        core.work.notify_all();
        self.inner.event.notify();
        true
    }

    pub(crate) fn elect_copy(&self) -> bool {
        self.elect(OriginalImageState::PreparedCopy)
    }
    pub(crate) fn elect_save(&self) -> bool {
        self.elect(OriginalImageState::PreparedSave)
    }

    pub(crate) fn take_copy(&self) -> Option<OriginalImageCopyEffect> {
        let core = self.inner.core.upgrade()?;
        let limits = self.clipboard_limits();
        let mut data = lock(&self.inner.data);
        if self.inner.request != OriginalImageRequest::Copy
            || data.state != OriginalImageState::Publishing
        {
            return None;
        }
        Some(OriginalImageCopyEffect::new(
            core,
            self.inner.clone(),
            data.copy.take()?,
            limits,
        ))
    }

    pub(crate) fn cancel(&self) {
        cancel(&self.inner, false);
    }
}

impl Drop for OriginalImageCommand {
    fn drop(&mut self) {
        cancel(&self.inner, true);
    }
}

fn release_if_done(
    state: &mut ProcessState,
    operation: &Arc<OriginalCommandState>,
    data: &CommandData,
) -> bool {
    if !data.worker_active
        && !data.picker_pending
        && matches!(
            data.state,
            OriginalImageState::Available(_) | OriginalImageState::Settled(_)
        )
        && (operation.request == OriginalImageRequest::Availability
            || data.delivered
            || !data.handle_alive)
        && state
            .original
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, operation))
    {
        state.original.take();
        return true;
    }
    false
}

fn cancel(operation: &Arc<OriginalCommandState>, dropping: bool) {
    let Some(core) = operation.core.upgrade() else {
        return;
    };
    let mut state = lock(&core.state);
    let mut data = lock(&operation.data);
    if dropping {
        data.handle_alive = false;
    }
    if data.state == OriginalImageState::Publishing {
        if dropping && operation.request == OriginalImageRequest::Copy && data.copy.is_some() {
            data.copy.take();
            data.encoded_bytes = 0;
            data.state = OriginalImageState::Settled(Err(OriginalImageFailure::Cancelled));
        }
    } else if matches!(data.state, OriginalImageState::Settled(_)) && !operation.live() {
        data.delivered = true;
    } else if !matches!(
        data.state,
        OriginalImageState::Settled(_) | OriginalImageState::Available(_)
    ) {
        operation.cancellation.cancel();
        data.copy.take();
        data.encoded_bytes = 0;
        data.choice.take();
        data.state = OriginalImageState::Settled(Err(OriginalImageFailure::Cancelled));
        state.cancellations = state.cancellations.saturating_add(1);
    }
    if !data.worker_active && operation.cancellation.is_cancelled() {
        data.delivered = true;
    }
    let released = release_if_done(&mut state, operation, &data);
    drop(data);
    drop(state);
    operation.event.notify();
    if let Some(attachment) = operation.attachment.upgrade() {
        attachment.event.notify();
    }
    core.work.notify_all();
    core.event.notify();
    if released {
        core.original_capacity.notify();
    }
}

pub(super) fn retire_window(window: &Arc<WindowState>) {
    let operation = window
        .core
        .upgrade()
        .and_then(|core| lock(&core.state).original.clone());
    if let Some(operation) =
        operation.filter(|operation| Weak::ptr_eq(&operation.window, &Arc::downgrade(window)))
    {
        cancel(&operation, false);
    }
    if let Some(core) = window.core.upgrade() {
        core.original_capacity.notify();
    }
}

pub(super) fn has_work(state: &ProcessState) -> bool {
    state.original.as_ref().is_some_and(|operation| {
        let data = lock(&operation.data);
        data.state == OriginalImageState::Preparing && !data.worker_active
    })
}

pub(super) fn claim(
    core: &Arc<ProcessCore>,
) -> Option<(Arc<OriginalCommandState>, Arc<AttachmentState>)> {
    let mut state = lock(&core.state);
    if state.stopping {
        return None;
    }
    let operation = state.original.clone()?;
    let mut data = lock(&operation.data);
    if data.state != OriginalImageState::Preparing || data.worker_active {
        return None;
    }
    let attachment = operation.attachment.upgrade()?;
    if !operation.live() {
        return None;
    }
    data.worker_active = true;
    state.active_workers += 1;
    attachment.active_workers.fetch_add(1, Ordering::AcqRel);
    drop(data);
    Some((operation, attachment))
}

pub(super) fn run(
    core: &Arc<ProcessCore>,
    operation: Arc<OriginalCommandState>,
    attachment: Arc<AttachmentState>,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        preparation::run(core, &operation, &attachment)
    }))
    .unwrap_or(Err(OriginalImageFailure::Worker));
    let mut state = lock(&core.state);
    let mut data = lock(&operation.data);
    if let Err(error) = result {
        data.copy.take();
        data.encoded_bytes = 0;
        data.state = OriginalImageState::Settled(Err(error));
        state.failures = state.failures.saturating_add(1);
    }
    data.worker_active = false;
    if operation.cancellation.is_cancelled() || attachment.retired.load(Ordering::Acquire) {
        data.delivered = true;
    }
    state.active_workers -= 1;
    attachment.active_workers.fetch_sub(1, Ordering::AcqRel);
    let released = release_if_done(&mut state, &operation, &data);
    drop(data);
    drop(state);
    operation.event.notify();
    attachment.event.notify();
    core.event.notify();
    if released {
        core.original_capacity.notify();
    }
}
