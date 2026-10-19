use super::*;
use crate::activity_service::ActivityHeightRead;
use crate::widgets::activity_panel::model::Feedback;

impl MainWindowShellRoot {
    pub(super) fn activity_frame(
        &mut self,
        frame: actor::Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<ActivityHeightRead>, bool, bool) {
        let actor::Frame {
            work,
            step: _,
            snapshot,
            facts,
            pages,
            retry,
            failure,
            retrying,
        } = frame;
        let mut retained = None;
        let mut reload = false;
        match work {
            actor::Work::HeightRead(Ok(read)) => {
                match read.with_current(|height| {
                    let value = height
                        .map(|height| height.get())
                        .unwrap_or(model::DEFAULT_HEIGHT);
                    self.activity.committed_height = value;
                    if !self.activity.resizing {
                        self.activity.preferred_height = value;
                    }
                }) {
                    Ok(()) => cx.notify(),
                    Err(ActivityReadError::Observation(
                        beryl_home_store::HomeObservedCoherenceError::Observation(
                            beryl_home_store::HomeMutationObservationError::Stale,
                        ),
                    )) => reload = true,
                    Err(error) if busy(&error) => retained = Some(read),
                    Err(_) => {}
                }
            }
            actor::Work::HeightRead(Err(error)) => {
                if matches!(&error, ActivityHeightError::Activity(error) if busy(error)) {
                    reload = true;
                } else {
                    self.report_activity_height("Saved activity panel height is unavailable", "Beryl could not read this window's saved height. Restore home access before resizing again.", window, cx);
                }
            }
            actor::Work::HeightCommit { value, result } => {
                let failure_notice = Some((
                    "Activity panel height was not saved",
                    "The window or home changed before saving. Resize again to retry.",
                ));
                let notice = match result {
                    Ok(flight) => {
                        let notice = flight.with_outcome(|result| match result {
                    Ok(ActivityHeightCommit::Committed {
                        later_failure: None,
                        ..
                    }) => {
                        self.activity.committed_height = value;
                        None
                    }
                    Ok(ActivityHeightCommit::Committed { .. }) => {
                        self.activity.committed_height = value;
                        Some((
                            "Activity panel height was saved",
                            "Beryl lost home access after saving. Restore home access before resizing again.",
                        ))
                    }
                    Ok(ActivityHeightCommit::Indeterminate { .. }) => Some((
                        "Activity panel height save is unresolved",
                        "Beryl could not confirm the saved height. Restore home access before resizing again.",
                    )),
                    _ => Some((
                        "Activity panel height was not saved",
                        "The window or home changed before saving. Resize again to retry.",
                    )),
                    }).unwrap_or(failure_notice);
                        self.activity.height_result = Some(flight);
                        notice
                    }
                    Err(_) => failure_notice,
                };
                if let Some((title, detail)) = notice {
                    self.report_activity_height(title, detail, window, cx);
                }
            }
            actor::Work::Step => {}
        }
        if let Some(snapshot) = snapshot {
            let head = snapshot.head();
            let query = model::Query {
                owner: self.activity.owner,
                period: head.work_period().get(),
                revision: head.revision().get(),
                total: head.logical_row_count(),
            };
            let interaction = snapshot.interaction_gate();
            let _ = snapshot.with_current(|snapshot| {
                self.publish_activity(
                    query,
                    pages,
                    interaction,
                    snapshot.auto_visible(),
                    retry,
                    failure,
                    retrying,
                    window,
                    cx,
                );
            });
        } else if let Some(facts) = facts {
            let interaction = facts.interaction_gate();
            let query = model::Query {
                owner: self.activity.owner,
                period: 0,
                revision: 0,
                total: 0,
            };
            let _ = facts.with_current(|facts| {
                self.publish_activity(
                    query,
                    Vec::new(),
                    interaction,
                    facts.auto_visible,
                    retry,
                    failure,
                    retrying,
                    window,
                    cx,
                );
            });
        }
        let blocked = self
            .activity
            .height_result
            .as_ref()
            .is_some_and(|flight| flight.blocks_replacement());
        if blocked {
            self.activity.height_command = None;
        }
        (retained, reload, blocked)
    }

    fn publish_activity(
        &mut self,
        query: model::Query,
        pages: Vec<Arc<model::Page>>,
        authority: ActivityInteractionGate,
        visible: bool,
        retry: Option<ActivityRetry>,
        failure: Option<Arc<str>>,
        retrying: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activity.authority = Some(Arc::new(authority.clone()));
        let gate = gate(authority);
        let new_failure = retry.as_ref().is_some_and(|retry| {
            self.activity
                .retry
                .as_ref()
                .is_none_or(|current| !current.same_request(retry))
        });
        if new_failure {
            let Some(key) = self.activity.feedback_key.checked_add(1) else {
                return;
            };
            self.activity.feedback_key = key;
        }
        if self.activity.resizing
            && self.activity.query.is_some_and(|current| {
                current.owner != query.owner || current.period != query.period
            })
        {
            self.restore_activity_resize();
        }
        self.activity.retry = retry;
        self.activity.query = Some(query);
        self.activity.gate = Some(gate.clone());
        self.activity.visible = visible;
        if !visible {
            if let Some(widget) = self.activity.widget.clone() {
                widget.update(cx, |widget, cx| widget.set_inert(window, cx));
            }
            if self.activity.resizing {
                self.restore_activity_resize();
            }
            self.activity.widget = None;
            self.activity.subscription = None;
            cx.notify();
            return;
        }
        if self.activity.widget.is_none() {
            let safe = self.notice_safe_focus(cx);
            let height = self.activity.preferred_height;
            let widget = cx.new(|cx| {
                let mut widget = ActivityPanel::new(safe, window, cx);
                widget.allocation(height, 320., cx);
                widget
            });
            self.activity.subscription =
                Some(
                    cx.subscribe_in(&widget, window, |root, widget, event, window, cx| {
                        root.activity_event(&widget, event.clone(), window, cx);
                    }),
                );
            self.activity.widget = Some(widget);
        }
        let widget = self.activity.widget.as_ref().unwrap().clone();
        let appearance = self
            .controller
            .as_ref()
            .map(|controller| controller.appearance.generation.clone());
        let feedback = failure.map(|explanation| Feedback {
            identity: self.activity.feedback_key,
            explanation,
            pending: retrying,
            retry_available: !retrying && self.activity.retry.is_some(),
        });
        widget.update(cx, |widget, cx| {
            if let Some(appearance) = appearance {
                widget.set_appearance(appearance, cx);
            }
            if widget.replace(query, pages, gate, window, cx).is_ok() {
                widget.feedback(feedback, window, cx);
            }
        });
        cx.notify();
    }

    fn report_activity_height(
        &mut self,
        title: &'static str,
        detail: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(revision) = self.activity.height_notice_revision.checked_add(1) else {
            return;
        };
        self.activity.height_notice_revision = revision;
        self.activity_height_notice(
            self.activity.height_condition.clone(),
            revision,
            title,
            detail,
            window,
            cx,
        );
    }
}
