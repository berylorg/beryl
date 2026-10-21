use super::*;

pub(crate) struct OriginalImageCopyEffect {
    core: Arc<ProcessCore>,
    operation: Arc<OriginalCommandState>,
    item: Option<ClipboardItem>,
    limits: ClipboardLimits,
    invoked: bool,
    settled: bool,
}

impl OriginalImageCopyEffect {
    pub(super) fn new(
        core: Arc<ProcessCore>,
        operation: Arc<OriginalCommandState>,
        item: ClipboardItem,
        limits: ClipboardLimits,
    ) -> Self {
        Self {
            core,
            operation,
            item: Some(item),
            limits,
            invoked: false,
            settled: false,
        }
    }

    pub(crate) fn publish(
        mut self,
        write: impl FnOnce(&ClipboardItem, ClipboardLimits) -> Result<(), gpui::ClipboardError>,
    ) -> Result<(), OriginalImageFailure> {
        self.invoked = true;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write(
                self.item.as_ref().expect("owned original copy output"),
                self.limits,
            )
        }))
        .unwrap_or(Err(gpui::ClipboardError::Ownership))
        .map_err(|_| OriginalImageFailure::Clipboard);
        self.finish(result);
        result
    }

    fn finish(&mut self, result: Result<(), OriginalImageFailure>) {
        self.item.take();
        let mut state = lock(&self.core.state);
        let mut data = lock(&self.operation.data);
        data.encoded_bytes = 0;
        data.encoded_length = 0;
        data.state = OriginalImageState::Settled(result);
        if !data.handle_alive || !self.operation.live() {
            data.delivered = true;
        }
        let released = release_if_done(&mut state, &self.operation, &data);
        self.settled = true;
        drop(data);
        drop(state);
        self.operation.event.notify();
        if let Some(attachment) = self.operation.attachment.upgrade() {
            attachment.event.notify();
        }
        self.core.event.notify();
        if released {
            self.core.original_capacity.notify();
        }
    }
}

impl Drop for OriginalImageCopyEffect {
    fn drop(&mut self) {
        if !self.settled {
            self.finish(Err(if self.invoked {
                OriginalImageFailure::Clipboard
            } else {
                OriginalImageFailure::Cancelled
            }));
        }
    }
}
