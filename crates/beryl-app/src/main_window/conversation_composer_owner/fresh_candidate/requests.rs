use super::*;

pub(crate) struct MainWindowFreshClaimWidgetBatch {
    close: MainWindowConversationComposerCloseTicket,
    sequence: u64,
    requests: Box<[RangeTextInputRequest]>,
}

pub(crate) struct MainWindowFreshClaimWidgetReply {
    batch: Arc<MainWindowFreshClaimWidgetBatch>,
    responses: VecDeque<MainWindowComposerDispatchOutcome>,
    delivered: usize,
    failure: Option<String>,
    delivery_error: Option<Box<gpui_text_input::RangeTextInputError>>,
}

impl MainWindowFreshClaimWidgetBatch {
    pub(crate) fn window_id(&self) -> beryl_model::WindowId {
        self.close.selection().window_id()
    }

    pub(crate) fn close(&self) -> MainWindowConversationComposerCloseTicket {
        self.close
    }

    pub(crate) fn requests(&self) -> &[RangeTextInputRequest] {
        &self.requests
    }
}

impl MainWindowFreshClaimWidgetReply {
    pub(crate) fn pending(batch: Arc<MainWindowFreshClaimWidgetBatch>) -> Box<Self> {
        Box::new(Self {
            batch,
            responses: VecDeque::new(),
            delivered: 0,
            failure: None,
            delivery_error: None,
        })
    }

    pub(crate) fn batch(&self) -> &Arc<MainWindowFreshClaimWidgetBatch> {
        &self.batch
    }
    pub(crate) fn prepared(&self) -> usize {
        self.responses.len()
    }
    pub(crate) fn push(&mut self, response: MainWindowComposerDispatchOutcome) {
        self.responses.push_back(response);
    }
}

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn fresh_claim_widget_batch(
        &self,
        close: MainWindowConversationComposerCloseTicket,
    ) -> Result<Option<Arc<MainWindowFreshClaimWidgetBatch>>, String> {
        if !self.recovery_binding_current(close) || !self.shutdown_interaction_gated {
            return Err("fresh ordinary request collection binding changed".into());
        }
        let gui = self
            .fresh_recovery_gui
            .as_ref()
            .ok_or("fresh GUI preparation is missing")?;
        if !gui.ordinary_claim || gui.reply.is_some() {
            return Ok(None);
        }
        Ok(gui.batch.clone())
    }

    pub(in crate::main_window) fn accept_fresh_claim_widget_reply(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        reply: Box<MainWindowFreshClaimWidgetReply>,
    ) -> Result<(), (Box<MainWindowFreshClaimWidgetReply>, String)> {
        let current = self.recovery_binding_current(close) && self.shutdown_interaction_gated;
        let Some(gui) = self.fresh_recovery_gui.as_mut() else {
            return Err((reply, "fresh ordinary GUI preparation is missing".into()));
        };
        if !current
            || !gui.ordinary_claim
            || gui.reply.is_some()
            || gui
                .batch
                .as_ref()
                .is_none_or(|batch| !Arc::ptr_eq(batch, &reply.batch))
            || reply.batch.close != close
            || reply.batch.sequence != gui.request_sequence
            || reply.responses.len() != reply.batch.requests.len()
            || reply.delivered != 0
        {
            return Err((
                reply,
                "fresh ordinary reply belongs to a stale request batch".into(),
            ));
        }
        gui.reply = Some(reply);
        Ok(())
    }

    pub(super) fn advance_ordinary_fresh_requests(
        &mut self,
        close: MainWindowConversationComposerCloseTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if !self.recovery_binding_current(close)
            || !self.shutdown_interaction_gated
            || self.fresh_recovery_release_requests.is_some()
            || self.bound_service()?.selected_identity() != Some(self.selection)
        {
            return Err("fresh ordinary response delivery binding changed".into());
        }
        let gui = self.fresh_recovery_gui.as_mut().unwrap();
        if let Some(reply) = gui.reply.as_mut() {
            if let Some(failure) = &reply.failure {
                return Err(failure.clone());
            }
            while let Some(response) = reply.responses.front() {
                let request = &reply.batch.requests[reply.delivered];
                let compatible = match (request, response) {
                    (
                        RangeTextInputRequest::Page(request),
                        MainWindowComposerDispatchOutcome::Page(page),
                    ) => page.key() == request.key(),
                    (
                        RangeTextInputRequest::ObjectPage(request),
                        MainWindowComposerDispatchOutcome::ObjectPage(page),
                    ) => page.key() == request.key(),
                    (
                        RangeTextInputRequest::CancelPage(_)
                        | RangeTextInputRequest::ReleasePage(_)
                        | RangeTextInputRequest::CancelObjectPage(_)
                        | RangeTextInputRequest::ReleaseObjectPage(_),
                        MainWindowComposerDispatchOutcome::Released,
                    ) => true,
                    _ => false,
                };
                if !compatible {
                    let error =
                        "fresh ordinary response does not match its original request".to_owned();
                    reply.failure = Some(error.clone());
                    return Err(error);
                }
                let response = reply.responses.pop_front().unwrap();
                let result = match (request, response) {
                    (
                        RangeTextInputRequest::Page(request),
                        MainWindowComposerDispatchOutcome::Page(page),
                    ) if page.key() == request.key() => self
                        .input
                        .update(cx, |input, cx| input.deliver_page(page, window, cx)),
                    (
                        RangeTextInputRequest::ObjectPage(request),
                        MainWindowComposerDispatchOutcome::ObjectPage(page),
                    ) if page.key() == request.key() => self.input.update(cx, |input, cx| {
                        input.deliver_object_page_in_window(page, window, cx)
                    }),
                    (
                        RangeTextInputRequest::CancelPage(_)
                        | RangeTextInputRequest::ReleasePage(_)
                        | RangeTextInputRequest::CancelObjectPage(_)
                        | RangeTextInputRequest::ReleaseObjectPage(_),
                        MainWindowComposerDispatchOutcome::Released,
                    ) => Ok(()),
                    _ => unreachable!(),
                };
                if let Err(error) = result
                    && !matches!(
                        error,
                        gpui_text_input::RangeTextInputError::PageResponseRejected(_)
                            | gpui_text_input::RangeTextInputError::ObjectResponseRejected(_)
                    )
                {
                    let detail = format!(
                        "fresh ordinary response delivery failed at batch {}, ordinal {}: {}",
                        reply.batch.sequence,
                        reply.delivered,
                        fresh_response_error_kind(&error),
                    );
                    reply.delivery_error = Some(Box::new(error));
                    reply.failure = Some(detail.clone());
                    return Err(detail);
                }
                reply.delivered += 1;
            }
            if reply.delivered != reply.batch.requests.len() {
                return Err("fresh ordinary reply delivery remains incomplete".into());
            }
            gui.reply.take();
            gui.batch.take();
        }
        if gui.batch.is_some() {
            return Ok(false);
        }
        let next_sequence = gui
            .request_sequence
            .checked_add(1)
            .ok_or("fresh ordinary request identity exhausted")?;
        let mut requests = Vec::with_capacity(16);
        for _ in 0..16 {
            let Some(request) = self.input.update(cx, |input, _| input.take_request()) else {
                break;
            };
            requests.push(request);
        }
        if requests.is_empty() {
            return Ok(true);
        }
        gui.request_sequence = next_sequence;
        gui.batch = Some(Arc::new(MainWindowFreshClaimWidgetBatch {
            close,
            sequence: gui.request_sequence,
            requests: requests.into_boxed_slice(),
        }));
        Ok(false)
    }
}

fn fresh_response_error_kind(error: &gpui_text_input::RangeTextInputError) -> &'static str {
    use gpui_text_input::RangeTextInputError::*;
    match error {
        InvalidLimits => "InvalidLimits",
        NotMounted => "NotMounted",
        Busy => "Busy",
        Pending => "Pending",
        ReadOnly => "ReadOnly",
        UnsupportedMutationKind => "UnsupportedMutationKind",
        Stale => "Stale",
        MalformedSeed => "MalformedSeed",
        NotQuiescent => "NotQuiescent",
        SurfaceCapacity => "SurfaceCapacity",
        PageResponseCapacity(_) => "PageResponseCapacity",
        ObjectResponseCapacity(_) => "ObjectResponseCapacity",
        PageResponseRejected(_) => "PageResponseRejected",
        ObjectResponseRejected(_) => "ObjectResponseRejected",
        DetachedCapacity => "DetachedCapacity",
        IncompleteSurface => "IncompleteSurface",
        Geometry(_) => "Geometry",
        Mutation(_) => "Mutation",
        Contract(_) => "Contract",
        Clipboard(_) => "Clipboard",
        _ => "Other",
    }
}
