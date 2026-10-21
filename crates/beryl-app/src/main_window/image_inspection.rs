use std::{cell::Cell, rc::Rc, sync::Arc};

use gpui::{AnyElement, Context, ImageResourcePin, IntoElement, Window, canvas, div, prelude::*};

use crate::image_processing::{
    ImageMediaChanged, ImageMediaFailure, ImageMediaInterest, ImageMediaWindow, ImageSourceDemand,
};

pub(crate) struct ImageInspection {
    demand: Option<ImageSourceDemand>,
    interest: Option<ImageMediaInterest>,
    pin: Option<Arc<ImageResourcePin>>,
    unavailable: bool,
    native_failure: Option<ImageMediaFailure>,
    paint_failure: Rc<Cell<Option<gpui::ImageResourceError>>>,
    #[cfg(any(test, feature = "test-faults"))]
    painted: Rc<Cell<bool>>,
    wait: Option<(gpui::Task<()>, Rc<Cell<bool>>)>,
}

impl ImageInspection {
    pub(crate) fn new(media: Option<&ImageMediaWindow>, demand: Option<ImageSourceDemand>) -> Self {
        let interest = media
            .zip(demand)
            .and_then(|(media, demand)| media.request(demand).ok());
        let unavailable = interest.is_none();
        Self {
            demand,
            interest,
            pin: None,
            unavailable,
            native_failure: None,
            paint_failure: Rc::new(Cell::new(None)),
            #[cfg(any(test, feature = "test-faults"))]
            painted: Rc::new(Cell::new(false)),
            wait: None,
        }
    }

    pub(crate) fn prepare<T: 'static>(
        &mut self,
        media: Option<&ImageMediaWindow>,
        window: &mut Window,
        cx: &mut Context<T>,
    ) {
        let paint_failure = self.paint_failure.take();
        let invalidated = self.pin.as_ref().is_some_and(|pin| !pin.is_current());
        if invalidated || paint_failure.is_some() {
            self.pin = None;
            self.wait = None;
            #[cfg(any(test, feature = "test-faults"))]
            self.painted.set(false);
            self.unavailable = !invalidated;
            if invalidated {
                self.interest = None;
                self.native_failure = None;
                self.interest = media
                    .filter(|media| !media.is_closed())
                    .zip(self.demand)
                    .and_then(|(media, demand)| media.request(demand).ok());
                self.unavailable = self.interest.is_none();
            } else if let Some(error) = paint_failure {
                self.native_failure = Some(ImageMediaFailure::Native(error));
            }
        }
        if let Some(failure) = self.native_failure {
            let Some(interest) = self
                .interest
                .as_ref()
                .filter(|interest| interest.native_retry_allowed(failure))
            else {
                self.interest = None;
                self.wait = None;
                self.native_failure = None;
                self.unavailable = true;
                return;
            };
            if interest.native_retry_ready() {
                self.wait = None;
                self.interest = None;
                self.interest = media
                    .zip(self.demand)
                    .and_then(|(media, demand)| media.request(demand).ok());
                self.native_failure = None;
                self.unavailable = self.interest.is_none();
            } else {
                let changed = interest.native_retry_changed();
                self.wait_for(changed, cx);
                return;
            }
        }
        if self.unavailable || self.pin.is_some() {
            return;
        }
        let Some(interest) = self.interest.as_ref() else {
            return;
        };
        let changed = interest.changed();
        match media.map(|media| media.prepare_ready(interest, window)) {
            Some(Ok(Some(pin))) => {
                self.pin = Some(Arc::new(pin));
                self.wait = None;
            }
            Some(Ok(None)) => {
                self.wait_for(changed, cx);
            }
            Some(Err(error)) if interest.native_retry_allowed(error.0) => {
                self.unavailable = true;
                self.native_failure = Some(error.0);
                self.wait = None;
                let changed = interest.native_retry_changed();
                self.wait_for(changed, cx);
            }
            _ => {
                self.unavailable = true;
                self.interest = None;
                self.wait = None;
            }
        }
    }

    fn wait_for<T: 'static>(&mut self, changed: ImageMediaChanged, cx: &mut Context<T>) {
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

    pub(crate) fn element(&self) -> AnyElement {
        if let Some(pin) = self.pin.clone() {
            let paint_failure = self.paint_failure.clone();
            #[cfg(any(test, feature = "test-faults"))]
            let painted = self.painted.clone();
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let layout = pin.layout();
                    let scale = (f32::from(bounds.size.width) / layout.width() as f32)
                        .min(f32::from(bounds.size.height) / layout.height() as f32)
                        .min(1.0);
                    let size = gpui::size(
                        gpui::px(layout.width() as f32 * scale),
                        gpui::px(layout.height() as f32 * scale),
                    );
                    let offset = gpui::point(
                        (bounds.size.width - size.width) / 2.0,
                        (bounds.size.height - size.height) / 2.0,
                    );
                    let fitted = gpui::Bounds::new(bounds.origin + offset, size);
                    if let Err(error) =
                        window.paint_admitted_image(&pin, fitted, Default::default(), false)
                    {
                        paint_failure.set(Some(error));
                        window.refresh();
                    } else {
                        #[cfg(any(test, feature = "test-faults"))]
                        painted.set(true);
                    }
                },
            )
            .size_full()
            .into_any_element()
        } else {
            div()
                .child(if self.unavailable {
                    "This image is not available from local storage."
                } else {
                    "The local image presentation is still being prepared."
                })
                .into_any_element()
        }
    }

    #[cfg(any(test, feature = "test-faults"))]
    pub(crate) fn diagnostics(&self) -> (bool, bool, bool, bool) {
        (
            self.pin.as_ref().is_some_and(|pin| pin.is_current()),
            self.painted.get(),
            self.interest.is_some(),
            self.unavailable,
        )
    }

    #[cfg(test)]
    pub(crate) fn read_pixel_for_test(
        &self,
    ) -> Option<Result<[u8; 4], crate::image_processing::ImageMediaError>> {
        self.pin.as_ref().filter(|pin| pin.is_current())?;
        Some(self.interest.as_ref()?.read_pixel_for_test())
    }
}
