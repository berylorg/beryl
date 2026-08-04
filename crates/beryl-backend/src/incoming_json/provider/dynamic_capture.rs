use beryl_stream::PageLease;

use crate::{
    DynamicToolArgumentControl, DynamicToolArgumentFragment, DynamicToolArgumentScalarKind,
    DynamicToolCall, DynamicToolCallAbandonReason, DynamicToolCallError,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
    dynamic_tool::{DynamicToolCallIngress, dynamic_abandon_reason},
};

pub(super) struct DynamicToolCapture<'a> {
    sink: &'a mut dyn OrderedTurnStreamSink,
    ingress: DynamicToolCallIngress,
    page: Option<PageLease>,
    scalar_kind: Option<DynamicToolArgumentScalarKind>,
    scalar_offset: u64,
    active: bool,
    abandon: DynamicToolCallAbandonReason,
}

impl<'a> DynamicToolCapture<'a> {
    pub(super) fn begin(
        sink: &'a mut dyn OrderedTurnStreamSink,
        call: DynamicToolCall,
        ingress: DynamicToolCallIngress,
    ) -> Result<Self, DynamicToolCallError> {
        submit_applied(sink, OrderedTurnStreamOperation::DynamicBegin(call))?;
        Ok(Self {
            sink,
            ingress,
            page: None,
            scalar_kind: None,
            scalar_offset: 0,
            active: true,
            abandon: DynamicToolCallAbandonReason::SchemaFailure,
        })
    }

    pub(super) fn control(
        &mut self,
        control: DynamicToolArgumentControl,
    ) -> Result<(), DynamicToolCallError> {
        submit_applied(
            self.sink,
            OrderedTurnStreamOperation::DynamicArgumentControl(control),
        )
        .inspect_err(|error| self.record_error(error))
    }

    pub(super) fn begin_scalar(
        &mut self,
        kind: DynamicToolArgumentScalarKind,
    ) -> Result<(), DynamicToolCallError> {
        self.control(DynamicToolArgumentControl::ScalarStart(kind))?;
        self.scalar_kind = Some(kind);
        self.scalar_offset = 0;
        self.acquire_empty_page()
    }

    pub(super) fn output_window(&mut self) -> Result<&mut [u8], DynamicToolCallError> {
        if self.page.is_none() {
            self.acquire_empty_page()?;
        }
        if self.page.as_ref().is_some_and(page_needs_exchange) {
            self.exchange_page()?;
        }
        let page = self
            .page
            .as_mut()
            .expect("dynamic capture owns one output page");
        let start = page.len();
        Ok(&mut page.buffer_mut()[start..])
    }

    pub(super) fn commit_output(&mut self, produced: usize) -> Result<(), DynamicToolCallError> {
        let page = self.page.as_mut().expect("output window acquired a page");
        let length = page
            .len()
            .checked_add(produced)
            .filter(|length| *length <= page.capacity())
            .ok_or_else(invalid_control)?;
        page.set_len(length).map_err(|_| invalid_control())?;
        Ok(())
    }

    pub(super) fn flush_if_full(&mut self) -> Result<(), DynamicToolCallError> {
        if self.page.as_ref().is_some_and(page_needs_exchange) {
            self.exchange_page()?;
        }
        Ok(())
    }

    pub(super) fn flush_nonempty(&mut self) -> Result<(), DynamicToolCallError> {
        if self.page.as_ref().is_some_and(|page| !page.is_empty()) {
            self.exchange_page()?;
        }
        Ok(())
    }

    pub(super) fn end_scalar(
        &mut self,
        kind: DynamicToolArgumentScalarKind,
    ) -> Result<(), DynamicToolCallError> {
        if self.scalar_kind != Some(kind) {
            return Err(invalid_control());
        }
        if self.page.as_ref().is_some_and(|page| !page.is_empty()) {
            self.exchange_page()?;
        }
        drop(self.page.take());
        self.scalar_kind = None;
        self.control(DynamicToolArgumentControl::ScalarEnd(kind))
    }

    fn acquire_empty_page(&mut self) -> Result<(), DynamicToolCallError> {
        let page = match self
            .sink
            .submit(OrderedTurnStreamOperation::DynamicAcquirePage)
        {
            Ok(OrderedTurnStreamCompletion::PageLease(page)) => page,
            Ok(OrderedTurnStreamCompletion::Applied | OrderedTurnStreamCompletion::Approval(_)) => {
                self.abandon = DynamicToolCallAbandonReason::SinkRejected;
                return Err(DynamicToolCallError::UnexpectedCompletion);
            }
            Err(error) => {
                let cause = error.cause();
                self.abandon = dynamic_abandon_reason(cause);
                return Err(cause.into());
            }
        };
        if !page.is_empty() || page.capacity() < bounded_json::MIN_OUTPUT_CAPACITY {
            self.page = Some(page);
            self.abandon = DynamicToolCallAbandonReason::SinkRejected;
            return Err(invalid_control());
        }
        self.page = Some(page);
        Ok(())
    }

    fn exchange_page(&mut self) -> Result<(), DynamicToolCallError> {
        let page = self
            .page
            .take()
            .expect("dynamic capture owns one current page");
        if page.is_empty() {
            self.page = Some(page);
            return Ok(());
        }
        let kind = self
            .scalar_kind
            .expect("a nonempty dynamic page exists only for an open scalar");
        let bytes = u64::try_from(page.len()).map_err(|_| invalid_control())?;
        let next_offset = self
            .scalar_offset
            .checked_add(bytes)
            .ok_or_else(invalid_control)?;
        let operation = OrderedTurnStreamOperation::DynamicArgumentFragment(
            DynamicToolArgumentFragment::new(kind, self.scalar_offset, page),
        );
        match self.sink.submit(operation) {
            Ok(OrderedTurnStreamCompletion::PageLease(next))
                if next.is_empty() && next.capacity() >= bounded_json::MIN_OUTPUT_CAPACITY =>
            {
                self.page = Some(next);
                self.scalar_offset = next_offset;
                Ok(())
            }
            Ok(OrderedTurnStreamCompletion::PageLease(next)) => {
                self.page = Some(next);
                self.abandon = DynamicToolCallAbandonReason::SinkRejected;
                Err(invalid_control())
            }
            Ok(OrderedTurnStreamCompletion::Applied | OrderedTurnStreamCompletion::Approval(_)) => {
                self.abandon = DynamicToolCallAbandonReason::SinkRejected;
                Err(DynamicToolCallError::UnexpectedCompletion)
            }
            Err(error) => {
                let (operation, cause) = error.into_parts();
                let OrderedTurnStreamOperation::DynamicArgumentFragment(returned) = operation
                else {
                    unreachable!("sink returned a different operation than it received")
                };
                self.page = Some(returned.into_lease());
                self.abandon = dynamic_abandon_reason(cause);
                Err(cause.into())
            }
        }
    }

    pub(super) fn seal(mut self) -> Result<(), DynamicToolCallError> {
        drop(self.page.take());
        submit_applied(self.sink, OrderedTurnStreamOperation::DynamicSeal)
            .inspect_err(|error| self.record_error(error))?;
        self.ingress.seal();
        self.active = false;
        Ok(())
    }

    pub(super) fn mark_transport_lost(&mut self) {
        self.abandon = DynamicToolCallAbandonReason::TransportLost;
    }

    fn record_error(&mut self, error: &DynamicToolCallError) {
        if let DynamicToolCallError::Submit(cause) = error {
            self.abandon = dynamic_abandon_reason(*cause);
        }
    }
}

impl Drop for DynamicToolCapture<'_> {
    fn drop(&mut self) {
        drop(self.page.take());
        if self.active {
            self.ingress.abandon();
            let _ = self
                .sink
                .submit(OrderedTurnStreamOperation::DynamicAbandon(self.abandon));
        }
    }
}

fn page_needs_exchange(page: &PageLease) -> bool {
    !page.is_empty() && page.capacity() - page.len() < bounded_json::MIN_OUTPUT_CAPACITY
}

fn invalid_control() -> DynamicToolCallError {
    OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl).into()
}

fn submit_applied(
    sink: &mut dyn OrderedTurnStreamSink,
    operation: OrderedTurnStreamOperation,
) -> Result<(), DynamicToolCallError> {
    match sink.submit(operation) {
        Ok(OrderedTurnStreamCompletion::Applied) => Ok(()),
        Ok(
            OrderedTurnStreamCompletion::PageLease(_) | OrderedTurnStreamCompletion::Approval(_),
        ) => Err(DynamicToolCallError::UnexpectedCompletion),
        Err(error) => Err(error.cause().into()),
    }
}
