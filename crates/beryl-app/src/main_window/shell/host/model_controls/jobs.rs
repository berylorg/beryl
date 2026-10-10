use super::*;
use crate::model_selection::menu::{ModelPageArrival, read_model_range};

impl MainWindowShellRoot {
    pub(super) fn start_model_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_controls.poll.is_some() {
            return;
        }
        let background = cx.background_executor().clone();
        self.model_controls.poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let observation = this.update_in(cx, |root, window, cx| {
                    root.sync_model_controls(window, cx);
                    Some((
                        root.model_controls.reader.clone()?,
                        root.model_controls.selection?,
                        root.model_controls.generation.load(Ordering::Acquire),
                        root.model_controls
                            .popup
                            .as_ref()
                            .filter(|popup| {
                                !popup.pending
                                    || popup.pending_publication.is_some()
                                    || popup.waiting_election
                            })
                            .map(|popup| {
                                (popup.serial, popup.scope.clone(), popup.preparation.clone())
                            }),
                    ))
                });
                let Ok(observation) = observation else {
                    break;
                };
                if let Some((reader, selection, generation, popup)) = observation {
                    let (result, validation) = background
                        .spawn(async move {
                            let validation = popup.map(|(serial, scope, preparation)| {
                                let validation = preparation
                                    .map(|fence| fence.revalidate())
                                    .unwrap_or(Ok(()))
                                    .and_then(|_| {
                                        scope
                                            .map(|scope| scope.query.revalidate())
                                            .unwrap_or(Ok(()))
                                    });
                                (serial, validation)
                            });
                            (reader.observe(selection), validation)
                        })
                        .await;
                    let _ = this.update_in(cx, |root, window, cx| {
                        if root.model_controls.generation.load(Ordering::Acquire) != generation
                            || root.status_selection(cx) != Some(selection)
                            || root
                                .model_controls
                                .reader
                                .as_ref()
                                .is_none_or(|reader| !reader.current())
                        {
                            return;
                        }
                        let previous = root.model_controls.values.clone();
                        let available = root.model_controls.runtime_available;
                        let resume = match validation {
                            Some((serial, Ok(()))) => {
                                root.model_controls.popup.as_ref().is_some_and(|popup| {
                                    popup.serial == serial && popup.waiting_election
                                })
                            }
                            Some((serial, Err(error))) => {
                                if error.scope_retired()
                                    && root
                                        .model_controls
                                        .popup
                                        .as_ref()
                                        .is_some_and(|popup| popup.serial == serial)
                                {
                                    root.close_model_menu(window, cx);
                                }
                                false
                            }
                            None => false,
                        };
                        match result {
                            Ok(scope) => {
                                let _ = scope.with_current_status(|| {
                                    root.model_controls.values = scope.values.clone();
                                    root.model_controls.runtime_available = true;
                                });
                            }
                            Err(
                                ModelReadError::Busy
                                | ModelReadError::Pending
                                | ModelReadError::Capacity,
                            ) => {}
                            Err(error) if error.runtime_unavailable() => {
                                root.model_controls.runtime_available = false
                            }
                            Err(_) => {}
                        }
                        if previous != root.model_controls.values
                            || available != root.model_controls.runtime_available
                        {
                            root.reconcile_model_menu_values(&previous, window, cx);
                            root.sync_model_controls(window, cx);
                            cx.notify();
                        }
                        if let Some(arrival) = root
                            .model_controls
                            .popup
                            .as_mut()
                            .and_then(|popup| popup.pending_publication.take())
                        {
                            root.apply_model_page(arrival, window, cx);
                        }
                        if resume {
                            if let Some(popup) = root.model_controls.popup.as_mut() {
                                popup.waiting_election = false;
                                popup.pending = false;
                            }
                            root.start_model_page(true, window, cx);
                        }
                    });
                }
                background.timer(Duration::from_millis(500)).await;
            }
        }));
    }

    pub(super) fn start_model_prepare(
        &mut self,
        reader: PublishedModelSelection,
        selection: MainWindowComposerSelectionIdentity,
        serial: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        let original = self
            .model_controls
            .popup
            .as_ref()
            .and_then(|popup| popup.preparation.clone());
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = background.spawn(async move {
                let preparation = match original { Some(fence) => fence, None => reader.preparation_fence(selection)? };
                let result = preparation.revalidate().and_then(|_| reader.prepare_fenced(selection, &preparation)).map(Arc::new);
                Ok::<_, ModelReadError>((preparation, result))
            }).await;
            let _ = this.update_in(cx, |root, window, cx| {
                let Some(popup) = root.model_controls.popup.as_mut().filter(|popup| popup.serial == serial) else { return; };
                if root.model_controls.selection != Some(selection) { return; }
                popup.pending = false;
                popup.retrying = false;
                let result = match result {
                    Ok((preparation, result)) => { popup.preparation = Some(preparation); result },
                    Err(_) => { root.close_model_menu(window, cx); return; },
                };
                match result {
                    Ok(scope) => {
                        if matches!(scope.query.with_current(|| ()), Err(error) if error.scope_retired()) { root.close_model_menu(window, cx); return; }
                        popup.scope = Some(scope);
                        popup.feedback = None;
                        root.start_model_page(false, window, cx);
                    },
                    Err(error) if error.scope_retired() => { root.close_model_menu(window, cx); return; },
                    Err(_) => popup.feedback = Some("Models could not be loaded."),
                }
                cx.notify();
            });
        });
        if let Some(popup) = self.model_controls.popup.as_mut() {
            popup.job = Some(task);
        }
    }

    pub(super) fn start_model_page(
        &mut self,
        retry: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.model_controls.popup.as_mut() else {
            return;
        };
        if popup.pending
            || popup.pending_publication.is_some()
            || (!retry && popup.failed_start.is_some())
        {
            return;
        }
        let Some(scope) = popup.scope.clone() else {
            return;
        };
        let locating_selected = popup.locating_selected;
        if !retry && !locating_selected && popup.collection.row(popup.desired).is_some() {
            return;
        }
        if !retry
            && !locating_selected
            && popup.collection.complete
            && popup.desired >= popup.collection.observed
        {
            return;
        }
        let (start, continuation) = if retry {
            (
                popup.failed_start.unwrap_or(0),
                popup.failed_continuation.clone(),
            )
        } else if locating_selected {
            (0, None)
        } else {
            popup.collection.request_for(popup.desired)
        };
        let target = if locating_selected {
            usize::MAX
        } else {
            popup.desired
        };
        popup.locating_selected = false;
        let retry_page = retry && popup.retry_page;
        let serial = popup.serial;
        let selected = self.model_controls.values.model.clone();
        popup.pending = true;
        popup.retrying = retry && popup.feedback.is_some();
        let background = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = background
                .spawn(async move {
                    read_model_range(
                        &scope.query,
                        start,
                        continuation,
                        target,
                        retry_page,
                        selected.as_deref(),
                    )
                })
                .await;
            let _ = this.update_in(cx, |root, window, cx| {
                let Some(popup) = root
                    .model_controls
                    .popup
                    .as_mut()
                    .filter(|popup| popup.serial == serial)
                else {
                    return;
                };
                popup.pending = false;
                popup.retrying = false;
                match result {
                    Ok(arrival) => {
                        root.apply_model_page(arrival, window, cx);
                        if locating_selected && retry && start > 0 {
                            if let Some(popup) = root.model_controls.popup.as_mut() {
                                popup.locating_selected = true;
                            }
                            root.start_model_page(false, window, cx);
                        }
                    }
                    Err(failure) => {
                        #[cfg(test)]
                        eprintln!(
                            "model page publication failure: kind={} retry_page={}",
                            crate::model_selection::menu::test_error_kind(&failure.error),
                            failure.retry_page
                        );
                        if failure.error.scope_retired() {
                            root.close_model_menu(window, cx);
                            return;
                        }
                        popup.failed_start = Some(failure.start);
                        popup.failed_continuation = failure.continuation;
                        popup.locating_selected |= locating_selected;
                        popup.retry_page = failure.retry_page;
                        if failure.waiting_election {
                            popup.waiting_election = true;
                            popup.pending = true;
                        } else {
                            popup.feedback = Some(if popup.collection.observed == 0 {
                                "Models could not be loaded."
                            } else {
                                "Model results are incomplete."
                            });
                        }
                    }
                }
                cx.notify();
            });
        });
        self.model_controls.popup.as_mut().unwrap().job = Some(task);
    }

    fn apply_model_page(
        &mut self,
        arrival: ModelPageArrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = arrival.page.clone();
        let mut arrival = Some(arrival);
        match page.with_current(|_| self.publish_model_page(arrival.take().unwrap(), window, cx)) {
            Ok(()) => {
                let activation = self
                    .model_controls
                    .popup
                    .as_mut()
                    .and_then(|popup| popup.activation.take());
                if let Some((index, record, effort, reasoning_activation)) = activation {
                    if self.model_controls.popup.as_ref().is_some_and(|popup| {
                        popup
                            .collection
                            .row(index)
                            .is_some_and(|(_, current)| current == &record)
                    }) {
                        self.activate_model_row_with_focus(
                            index,
                            effort,
                            reasoning_activation,
                            window,
                            cx,
                        );
                    }
                }
                self.start_model_page(false, window, cx);
                cx.notify();
            }
            Err(error) if error.scope_retired() => self.close_model_menu(window, cx),
            Err(_) => {
                if let Some(popup) = self.model_controls.popup.as_mut() {
                    popup.pending_publication = arrival;
                    popup.pending = true;
                }
            }
        }
    }

    fn publish_model_page(
        &mut self,
        arrival: ModelPageArrival,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let Some(popup) = self.model_controls.popup.as_mut() else {
            return;
        };
        popup.pending_publication = None;
        popup.pending = false;
        popup.waiting_election = false;
        popup.retrying = false;
        popup.feedback = None;
        popup.failed_start = None;
        popup.failed_continuation = None;
        popup.retry_page = false;
        if let Some(selected) = arrival.selected.filter(|(_, record)| {
            self.model_controls.values.model.as_deref() == Some(record.model.as_str())
        }) {
            popup.collection.selected = Some(selected);
        }
        popup.collection.install(
            arrival.start,
            arrival.page,
            self.model_controls.values.model.as_deref(),
        );
        if !popup.selected_revealed {
            if let Some((index, record)) = popup.collection.selected.clone() {
                popup.selected_revealed = true;
                if !popup.preserve_focus {
                    popup.collection.focused = Some((index, record));
                }
                popup.preserve_focus = false;
                popup.desired = index;
                popup.widget.reveal(index);
            } else if popup.collection.focused.is_none() && popup.collection.observed > 0 {
                popup.collection.reveal_focus(0);
            }
            if popup.collection.selected.is_none()
                && !popup.collection.complete
                && self.model_controls.values.model.is_some()
            {
                popup.desired = popup.collection.observed;
            }
        }
        let efforts = popup.efforts();
        let facts = rows::ModelMenuRows::new(
            &popup.collection,
            &self.model_controls.values,
            popup.pending,
            popup.feedback,
            efforts,
        );
        popup.widget.reconcile(&facts);
        if !popup.widget.has_focus() && popup.collection.observed > 0 {
            let index = popup
                .collection
                .focused
                .as_ref()
                .map(|(index, _)| *index)
                .unwrap_or(0);
            popup.widget.focus_row(index, &facts, false);
        }
    }
}
