use beryl_stream::PageLease;

use crate::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause, ProviderObservationAbandonReason,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationError,
    ProviderObservationFragment, ProviderObservationRoute, ProviderObservationSchemaError,
    ProviderValueContext, provider_observation::abandon_reason,
};

pub(super) struct ObservationCapture<'a> {
    sink: &'a mut dyn OrderedTurnStreamSink,
    begin: ProviderObservationBegin,
    page: Option<PageLease>,
    open_context: Option<ProviderValueContext>,
    active: bool,
    abandon: ProviderObservationAbandonReason,
}

impl<'a> ObservationCapture<'a> {
    pub(super) fn begin(
        sink: &'a mut dyn OrderedTurnStreamSink,
        begin: ProviderObservationBegin,
    ) -> Result<Self, ProviderObservationError> {
        submit_applied(sink, OrderedTurnStreamOperation::ProviderBegin(begin))?;
        Ok(Self {
            sink,
            begin,
            page: None,
            open_context: None,
            active: true,
            abandon: ProviderObservationAbandonReason::SchemaFailure,
        })
    }

    pub(super) const fn begin_kind(&self) -> ProviderObservationBegin {
        self.begin
    }

    pub(super) fn control(
        &mut self,
        control: ProviderObservationControl,
    ) -> Result<(), ProviderObservationError> {
        submit_applied(
            self.sink,
            OrderedTurnStreamOperation::ProviderControl(control),
        )
        .inspect_err(|error| {
            if let ProviderObservationError::Submit(cause) = error {
                self.abandon = abandon_reason(*cause);
            }
        })
    }

    pub(super) fn begin_text(
        &mut self,
        context: ProviderValueContext,
    ) -> Result<(), ProviderObservationError> {
        self.control(ProviderObservationControl::BeginField(context))?;
        self.open_context = Some(context);
        self.acquire_empty_page()
    }

    pub(super) fn output_window(&mut self) -> Result<&mut [u8], ProviderObservationError> {
        if self.page.is_none() {
            self.acquire_empty_page()?;
        }
        if self.page.as_ref().is_some_and(page_needs_exchange) {
            self.exchange_full_page()?;
        }
        let page = self.page.as_mut().expect("capture owns one output page");
        let start = page.len();
        Ok(&mut page.buffer_mut()[start..])
    }

    pub(super) fn commit_output(
        &mut self,
        produced: usize,
    ) -> Result<(), ProviderObservationError> {
        let page = self.page.as_mut().expect("output window acquired a page");
        let length = page
            .len()
            .checked_add(produced)
            .filter(|length| *length <= page.capacity())
            .ok_or_else(invalid_control)?;
        page.set_len(length).map_err(|_| invalid_control())?;
        Ok(())
    }

    pub(super) fn flush_if_full(&mut self) -> Result<(), ProviderObservationError> {
        if self.page.as_ref().is_some_and(page_needs_exchange) {
            self.exchange_full_page()?;
        }
        Ok(())
    }

    pub(super) fn write_fixed(&mut self, bytes: &[u8]) -> Result<(), ProviderObservationError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ProviderObservationSchemaError::InvalidString)?;
        let mut offset = 0;
        while offset < bytes.len() {
            let output = self.output_window()?;
            let proposed_end = offset + output.len().min(bytes.len() - offset);
            let end = (offset + 1..=proposed_end)
                .rev()
                .find(|end| text.is_char_boundary(*end))
                .expect("a minimum-size output window fits one UTF-8 scalar");
            let count = end - offset;
            output[..count].copy_from_slice(&bytes[offset..end]);
            self.commit_output(count)?;
            offset = end;
            self.flush_if_full()?;
        }
        Ok(())
    }

    pub(super) fn flush_nonempty(&mut self) -> Result<(), ProviderObservationError> {
        if self.page.as_ref().is_some_and(|page| !page.is_empty()) {
            self.exchange_full_page()?;
        }
        Ok(())
    }

    pub(super) fn end_text(
        &mut self,
        context: ProviderValueContext,
    ) -> Result<(), ProviderObservationError> {
        if self.page.as_ref().is_some_and(|page| !page.is_empty()) {
            self.exchange_full_page()?;
        }
        // No empty lease may remain held across a field boundary or the seal barrier.
        drop(self.page.take());
        self.open_context = None;
        self.control(ProviderObservationControl::EndField(context))
    }

    fn acquire_empty_page(&mut self) -> Result<(), ProviderObservationError> {
        let page = match self
            .sink
            .submit(OrderedTurnStreamOperation::ProviderAcquirePage)
        {
            Ok(OrderedTurnStreamCompletion::PageLease(page)) => page,
            Ok(OrderedTurnStreamCompletion::Applied | OrderedTurnStreamCompletion::Approval(_)) => {
                self.abandon = ProviderObservationAbandonReason::SinkRejected;
                return Err(ProviderObservationError::UnexpectedCompletion);
            }
            Err(error) => {
                let cause = error.cause();
                self.abandon = abandon_reason(cause);
                return Err(cause.into());
            }
        };
        if !page.is_empty() || page.capacity() < bounded_json::MIN_OUTPUT_CAPACITY {
            self.page = Some(page);
            self.abandon = ProviderObservationAbandonReason::SinkRejected;
            return Err(invalid_control());
        }
        self.page = Some(page);
        Ok(())
    }

    fn exchange_full_page(&mut self) -> Result<(), ProviderObservationError> {
        let page = self.page.take().expect("capture owns one current page");
        if page.is_empty() {
            self.page = Some(page);
            return Ok(());
        }
        let context = self
            .open_context
            .expect("a nonempty page exists only for an open field");
        let operation = OrderedTurnStreamOperation::ProviderFragment(
            ProviderObservationFragment::new(context, page),
        );
        match self.sink.submit(operation) {
            Ok(OrderedTurnStreamCompletion::PageLease(next))
                if next.is_empty() && next.capacity() >= bounded_json::MIN_OUTPUT_CAPACITY =>
            {
                self.page = Some(next);
                Ok(())
            }
            Ok(OrderedTurnStreamCompletion::PageLease(next)) => {
                self.page = Some(next);
                self.abandon = ProviderObservationAbandonReason::SinkRejected;
                Err(invalid_control())
            }
            Ok(OrderedTurnStreamCompletion::Applied | OrderedTurnStreamCompletion::Approval(_)) => {
                self.abandon = ProviderObservationAbandonReason::SinkRejected;
                Err(ProviderObservationError::UnexpectedCompletion)
            }
            Err(error) => {
                let (operation, cause) = error.into_parts();
                let OrderedTurnStreamOperation::ProviderFragment(returned) = operation else {
                    unreachable!("sink returned a different operation than it received")
                };
                self.page = Some(returned.into_lease());
                self.abandon = abandon_reason(cause);
                Err(cause.into())
            }
        }
    }

    pub(super) fn seal(
        mut self,
        route: ProviderObservationRoute,
    ) -> Result<(), ProviderObservationError> {
        drop(self.page.take());
        submit_applied(self.sink, OrderedTurnStreamOperation::ProviderSeal(route)).inspect_err(
            |error| {
                if let ProviderObservationError::Submit(cause) = error {
                    self.abandon = abandon_reason(*cause);
                }
            },
        )?;
        self.active = false;
        Ok(())
    }

    pub(super) fn mark_route_failure(&mut self) {
        self.abandon = ProviderObservationAbandonReason::MissingOrMalformedRoute;
    }

    pub(super) fn mark_transport_lost(&mut self) {
        self.abandon = ProviderObservationAbandonReason::TransportLost;
    }
}

impl Drop for ObservationCapture<'_> {
    fn drop(&mut self) {
        drop(self.page.take());
        if self.active {
            let _ = self
                .sink
                .submit(OrderedTurnStreamOperation::ProviderAbandon(self.abandon));
        }
    }
}

fn invalid_control() -> ProviderObservationError {
    OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl).into()
}

fn page_needs_exchange(page: &PageLease) -> bool {
    !page.is_empty() && page.capacity() - page.len() < bounded_json::MIN_OUTPUT_CAPACITY
}

fn submit_applied(
    sink: &mut dyn OrderedTurnStreamSink,
    operation: OrderedTurnStreamOperation,
) -> Result<(), ProviderObservationError> {
    match sink.submit(operation) {
        Ok(OrderedTurnStreamCompletion::Applied) => Ok(()),
        Ok(
            OrderedTurnStreamCompletion::PageLease(_) | OrderedTurnStreamCompletion::Approval(_),
        ) => Err(ProviderObservationError::UnexpectedCompletion),
        Err(error) => Err(error.cause().into()),
    }
}
