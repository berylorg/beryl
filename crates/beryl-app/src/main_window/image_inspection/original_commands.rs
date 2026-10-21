use crate::image_processing::{
    ImageMediaChanged, ImageMediaFailure, ImageMediaWindow, ImageSourceDemand,
    OriginalImageAvailability, OriginalImageCommand, OriginalImageFailure, OriginalImageRequest,
    OriginalImageState,
};
use gpui::{App, Context, FocusHandle, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Weak},
};

mod preview;
pub(crate) use preview::{PreviewImageCommands, composer_colors};

pub(crate) struct OriginalImageCommands {
    media: Weak<ImageMediaWindow>,
    demand: Option<ImageSourceDemand>,
    facts: Option<OriginalImageAvailability>,
    failure: Option<OriginalImageFailure>,
    active: Option<Rc<OriginalImageCommand>>,
    operation: OriginalImageRequest,
    wait: Option<(gpui::Task<()>, Rc<Cell<bool>>)>,
    waiting_capacity: bool,
    picker: Option<Picker>,
    published: Option<Rc<Cell<Option<Result<(), OriginalImageFailure>>>>>,
    pub(crate) feedback: &'static str,
}
struct Picker {
    result: Rc<Cell<Option<PickerResult>>>,
}
#[derive(Clone, Copy)]
enum PickerResult {
    Submitted,
    Cancelled,
    Failed,
}

impl OriginalImageCommands {
    pub(crate) fn new(media: Weak<ImageMediaWindow>, demand: Option<ImageSourceDemand>) -> Self {
        Self {
            media,
            demand,
            facts: None,
            failure: demand
                .is_none()
                .then_some(OriginalImageFailure::Unavailable),
            active: None,
            operation: OriginalImageRequest::Availability,
            wait: None,
            waiting_capacity: false,
            picker: None,
            published: None,
            feedback: "",
        }
    }
    pub(crate) fn picker_pending(&self) -> bool {
        self.picker.is_some()
    }
    pub(crate) fn reason(&self, command: OriginalImageRequest) -> Option<&'static str> {
        if self.active.is_some() || self.published.is_some() {
            return Some(if self.operation == OriginalImageRequest::Availability {
                "Checking the exact original image."
            } else {
                "An image command is in progress."
            });
        }
        if let Some(failure) = self.failure {
            if failure == OriginalImageFailure::Capacity {
                return Some("Original image command capacity is unavailable.");
            }
            return Some(failure_reason(failure));
        }
        let Some(facts) = self.facts else {
            return Some("Checking the exact original image.");
        };
        match command {
            OriginalImageRequest::Copy => facts.copy.err().map(failure_reason),
            OriginalImageRequest::Save => {
                (!facts.save).then_some("The original image is locally unavailable.")
            }
            OriginalImageRequest::Availability => Some("Checking the exact original image."),
        }
    }
    pub(crate) fn poll<T: 'static>(&mut self, cx: &mut Context<T>) {
        if let Some(result) = self.published.as_ref().and_then(|result| result.take()) {
            self.published = None;
            self.complete(result);
        }
        if let Some(picker) = self.picker.as_ref() {
            let result = picker.result.take();
            if let Some(result) = result {
                self.picker = None;
                match result {
                    PickerResult::Submitted => {}
                    PickerResult::Cancelled => {
                        self.active = None;
                        self.wait = None;
                        self.feedback = "Save cancelled.";
                    }
                    PickerResult::Failed => {
                        self.active = None;
                        self.wait = None;
                        self.feedback = "Save failed: the file picker is unavailable.";
                    }
                }
            }
        }
        if self.active.is_none()
            && self.facts.is_none()
            && self.failure.is_none()
            && self.published.is_none()
        {
            if self.waiting_capacity {
                if self
                    .wait
                    .as_ref()
                    .is_some_and(|(_, complete)| !complete.get())
                {
                    return;
                }
                self.wait = None;
                self.waiting_capacity = false;
            }
            let Some(media) = self.media.upgrade().filter(|media| !media.is_closed()) else {
                self.failure = Some(OriginalImageFailure::Unavailable);
                return;
            };
            let Some(demand) = self.demand else {
                return;
            };
            let capacity = match media.original_capacity_changed() {
                Ok(capacity) => capacity,
                Err(_) => {
                    self.failure = Some(OriginalImageFailure::Capacity);
                    return;
                }
            };
            match media.request_original(demand, OriginalImageRequest::Availability) {
                Ok(request) => {
                    self.active = Some(Rc::new(request));
                    self.operation = OriginalImageRequest::Availability;
                    self.wait = None;
                }
                Err(error) if error.0 == ImageMediaFailure::Capacity => {
                    self.waiting_capacity = true;
                    self.wait_for(capacity, cx);
                    return;
                }
                Err(_) => {
                    self.failure = Some(OriginalImageFailure::Unavailable);
                    return;
                }
            }
        }
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let changed = active.changed();
        match active.state() {
            OriginalImageState::Available(facts) => {
                self.facts = Some(facts);
                self.failure = None;
                self.active = None;
                self.wait = None;
            }
            OriginalImageState::Settled(result) => {
                self.complete(result);
                self.active = None;
                self.wait = None;
            }
            OriginalImageState::PreparedCopy => {
                if self.media.upgrade().is_none_or(|media| media.is_closed())
                    || !active.elect_copy()
                {
                    self.active = None;
                    self.wait = None;
                    self.complete(Err(OriginalImageFailure::Stale));
                    return;
                }
                let result = match active.take_copy() {
                    Some(effect) => {
                        effect.publish(|item, limits| write_clipboard(item, limits, cx))
                    }
                    None => Err(OriginalImageFailure::Clipboard),
                };
                self.complete(result);
                self.active = None;
                self.wait = None;
            }
            OriginalImageState::PreparedSave => {
                if self.media.upgrade().is_none_or(|media| media.is_closed())
                    || !active.elect_save()
                {
                    self.active = None;
                    self.wait = None;
                    self.complete(Err(OriginalImageFailure::Stale));
                    return;
                }
                let active = self.active.take().unwrap();
                self.wait = None;
                let result = Rc::new(Cell::new(None));
                self.published = Some(result.clone());
                cx.spawn(async move |owner, cx| {
                    loop {
                        let changed = active.changed();
                        if let OriginalImageState::Settled(value) = active.state() {
                            result.set(Some(value));
                            let _ = owner.update(cx, |_, cx| cx.notify());
                            break;
                        }
                        changed.await;
                    }
                })
                .detach();
            }
            OriginalImageState::WaitingForSaveChoice => {}
            _ => self.wait_for(changed, cx),
        }
    }
    fn complete(&mut self, result: Result<(), OriginalImageFailure>) {
        if self.operation == OriginalImageRequest::Availability {
            self.failure = result.err();
        } else {
            self.feedback = outcome(self.operation, result);
            if let Err(
                failure @ (OriginalImageFailure::Unavailable
                | OriginalImageFailure::Integrity
                | OriginalImageFailure::Stale),
            ) = result
            {
                self.facts = None;
                self.failure = Some(failure);
            }
        }
    }
    pub(crate) fn start<T: 'static>(
        &mut self,
        command: OriginalImageRequest,
        cx: &mut Context<T>,
    ) -> bool {
        if self.reason(command).is_some() {
            return false;
        }
        let request = self
            .media
            .upgrade()
            .zip(self.demand)
            .and_then(|(media, demand)| media.request_original(demand, command).ok());
        let Some(request) = request else {
            self.feedback = "Image command unavailable: capacity or source changed.";
            return false;
        };
        self.active = Some(Rc::new(request));
        self.operation = command;
        self.wait = None;
        self.feedback = if command == OriginalImageRequest::Copy {
            "Copying…"
        } else {
            "Choose a Save destination…"
        };
        if command == OriginalImageRequest::Save {
            let receiver = save_picker(cx);
            let result = Rc::new(Cell::new(None));
            let returned = result.clone();
            let command = self.active.as_ref().unwrap().clone();
            cx.spawn(async move |owner, cx| {
                let result = match receiver.await {
                    Ok(Ok(Some(choice))) => {
                        if command.submit_save_choice(choice) {
                            PickerResult::Submitted
                        } else {
                            PickerResult::Cancelled
                        }
                    }
                    Ok(Ok(None)) => {
                        command.picker_finished();
                        PickerResult::Cancelled
                    }
                    _ => {
                        command.fail_picker(OriginalImageFailure::Destination);
                        PickerResult::Failed
                    }
                };
                returned.set(Some(result));
                let _ = owner.update(cx, |_, cx| cx.notify());
            })
            .detach();
            self.picker = Some(Picker { result });
        }
        cx.notify();
        true
    }
    fn wait_for<T: 'static>(
        &mut self,
        changed: impl std::future::Future<Output = ()> + 'static,
        cx: &mut Context<T>,
    ) {
        if self
            .wait
            .as_ref()
            .is_some_and(|(_, complete)| complete.get())
        {
            self.wait = None;
        }
        if self.wait.is_none() {
            let complete = Rc::new(Cell::new(false));
            let finished = complete.clone();
            let task = cx.spawn(async move |owner, cx| {
                changed.await;
                finished.set(true);
                let _ = owner.update(cx, |_, cx| cx.notify());
            });
            self.wait = Some((task, complete));
        }
    }
}
impl Drop for OriginalImageCommands {
    fn drop(&mut self) {
        if let Some(active) = self.active.as_ref() {
            active.cancel();
        }
    }
}

fn write_clipboard(
    item: &gpui::ClipboardItem,
    limits: gpui::ClipboardLimits,
    cx: &mut App,
) -> Result<(), gpui::ClipboardError> {
    #[cfg(test)]
    {
        if let Some(writer) = cx.try_global::<TestOriginalClipboardWriter>() {
            let writer = writer.0.clone();
            return (writer.borrow_mut())(item, limits);
        }
        Err(gpui::ClipboardError::Unavailable)
    }
    #[cfg(not(test))]
    {
        cx.write_to_clipboard_checked(item, limits)
    }
}

#[cfg(test)]
pub(crate) struct TestOriginalClipboardWriter(
    pub(crate)  Rc<
        RefCell<
            Box<
                dyn FnMut(
                    &gpui::ClipboardItem,
                    gpui::ClipboardLimits,
                ) -> Result<(), gpui::ClipboardError>,
            >,
        >,
    >,
);
#[cfg(test)]
impl gpui::Global for TestOriginalClipboardWriter {}

#[cfg(test)]
pub(crate) struct TestOriginalSavePicker(
    pub(crate)  Rc<
        RefCell<
            Box<
                dyn FnMut() -> futures_channel::oneshot::Receiver<
                    gpui::Result<Option<gpui::SavePathChoice>>,
                >,
            >,
        >,
    >,
);
#[cfg(test)]
impl gpui::Global for TestOriginalSavePicker {}

fn save_picker(
    cx: &mut App,
) -> futures_channel::oneshot::Receiver<gpui::Result<Option<gpui::SavePathChoice>>> {
    #[cfg(test)]
    if let Some(picker) = cx.try_global::<TestOriginalSavePicker>() {
        let picker = picker.0.clone();
        return (picker.borrow_mut())();
    }
    cx.prompt_for_save_path(std::path::Path::new(""), Some("image"))
}

#[cfg(test)]
#[path = "../../../tests/unit/image_original_gui.rs"]
mod tests;
pub(crate) fn failure_reason(failure: OriginalImageFailure) -> &'static str {
    match failure {
        OriginalImageFailure::Unsupported => {
            "The original has no supported clipboard representation."
        }
        OriginalImageFailure::Capacity => "The complete original exceeds the clipboard capacity.",
        OriginalImageFailure::Integrity => "The original image failed integrity verification.",
        OriginalImageFailure::Cancelled | OriginalImageFailure::Stale => {
            "The exact image command is no longer current."
        }
        _ => "The original image is locally unavailable.",
    }
}
fn outcome(
    command: OriginalImageRequest,
    result: Result<(), OriginalImageFailure>,
) -> &'static str {
    match (command, result) {
        (OriginalImageRequest::Copy, Ok(())) => "Copied.",
        (OriginalImageRequest::Save, Ok(())) => "Saved.",
        (_, Err(OriginalImageFailure::Cancelled)) => "Image command cancelled.",
        (_, Err(OriginalImageFailure::Cleanup)) => "Save failed: temporary output cleanup failed.",
        (OriginalImageRequest::Copy, Err(_)) => "Copy failed.",
        _ => "Save failed.",
    }
}
