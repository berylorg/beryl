use super::*;

impl Ingester {
    pub(in super::super) fn passive_reply(
        &mut self,
        operation: BrokerOperation,
    ) -> (BrokerReply, bool) {
        use OrderedTurnStreamOperation as Op;
        let applied = || {
            (
                BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                false,
            )
        };
        match operation {
            BrokerOperation::SteeringMode => (
                BrokerReply::SteeringMode(Ok(SteeringUserMessageCaptureMode::Passive)),
                false,
            ),
            BrokerOperation::SteeringUnverified(message) => {
                self.passive.routed(
                    message.thread_id(),
                    message.turn_id(),
                    Some(OutageFact::UserCorrelation {
                        item: message.item_id(),
                        client: message.client_user_message_id(),
                    }),
                    true,
                );
                (BrokerReply::SteeringUnverified(Ok(())), false)
            }
            BrokerOperation::SteeringSelect(selection) => (
                BrokerReply::SteeringSelectionRejected(
                    selection,
                    OrderedTurnStreamSubmitCause::Unavailable,
                ),
                false,
            ),
            BrokerOperation::SteeringChecked(message) => {
                self.passive.routed(
                    message.thread_id(),
                    message.turn_id(),
                    Some(OutageFact::UserCorrelation {
                        item: message.item_id(),
                        client: message.client_user_message_id(),
                    }),
                    true,
                );
                (BrokerReply::SteeringChecked, false)
            }
            BrokerOperation::SteeringAbandon(_) => {
                self.passive.gap = true;
                (BrokerReply::SteeringAbandoned, false)
            }
            BrokerOperation::Ordered(operation) => match operation {
                Op::Approval(request) => (
                    BrokerReply::Applied(OrderedTurnStreamCompletion::Approval(
                        ApprovalOperationCompletion::Unanswered { request },
                    )),
                    false,
                ),
                Op::ProviderAcquirePage | Op::DynamicAcquirePage => match self.pages.try_lease() {
                    Ok(page) => (
                        BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                        false,
                    ),
                    Err(_) => (
                        BrokerReply::Rejected(
                            operation,
                            OrderedTurnStreamSubmitCause::CapacityFull,
                        ),
                        true,
                    ),
                },
                Op::ProviderBegin(begin) => {
                    self.passive.provider_open = true;
                    self.passive.discard_provider = false;
                    self.passive.offset = 0;
                    let mut bytes = [0; 16];
                    if getrandom::fill(&mut bytes).is_ok() {
                        self.passive.with_slot(|slot, ready| {
                            let _ =
                                slot.begin(ProviderObservationId::from_bytes(bytes), begin, ready);
                        });
                    } else {
                        self.passive.discard_provider = true;
                    }
                    applied()
                }
                Op::ProviderControl(control) => {
                    if matches!(control, ProviderObservationControl::BeginField(_)) {
                        self.passive.offset = 0;
                    }
                    if !self.passive.discard_provider {
                        self.passive.with_slot(|slot, _| {
                            let _ = slot.control(control);
                        });
                    }
                    applied()
                }
                Op::ProviderFragment(fragment) => {
                    if !self.passive.discard_provider {
                        let offset = self.passive.offset;
                        match std::str::from_utf8(fragment.bytes()) {
                            Ok(text) => self.passive.with_slot(|slot, _| {
                                let _ = slot.fragment(fragment.context(), offset, text);
                            }),
                            Err(_) => {
                                self.passive.discard_provider = true;
                            }
                        }
                        self.passive.offset =
                            self.passive.offset.saturating_add(fragment.bytes().len());
                    }
                    let mut page = fragment.into_lease();
                    page.clear();
                    (
                        BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                        false,
                    )
                }
                Op::ProviderSeal(route) => {
                    if self.passive.discard_provider {
                        self.passive
                            .routed(route.thread_id(), route.turn_id(), None, true);
                    } else {
                        self.passive.with_slot(|slot, ready| {
                            let _ = slot.seal(route, ready);
                        });
                    }
                    self.passive.provider_open = false;
                    self.passive.discard_provider = false;
                    applied()
                }
                Op::ProviderAbandon(_) => {
                    self.passive.with_slot(|slot, _| slot.abandon());
                    self.passive.provider_open = false;
                    self.passive.gap = true;
                    applied()
                }
                Op::DynamicArgumentFragment(fragment) => {
                    let mut page = fragment.into_lease();
                    page.clear();
                    (
                        BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                        false,
                    )
                }
                Op::NormalTurnTerminal(terminal) => {
                    self.passive.routed(
                        terminal.thread_id(),
                        terminal.turn_id(),
                        Some(OutageFact::Terminal(terminal.status())),
                        false,
                    );
                    applied()
                }
                Op::DynamicBegin(call) => {
                    drop(call);
                    self.passive.dynamic_open = true;
                    self.passive.gap = true;
                    applied()
                }
                Op::DynamicSeal | Op::DynamicAbandon(_) => {
                    self.passive.dynamic_open = false;
                    applied()
                }
                Op::DynamicArgumentControl(_) => applied(),
                Op::TurnStarted(message) => {
                    self.passive
                        .routed(message.thread_id(), message.turn_id(), None, true);
                    applied()
                }
                Op::CheckedUserMessage(message) => {
                    self.passive
                        .routed(message.thread_id(), message.turn_id(), None, true);
                    applied()
                }
                Op::ThreadStatusChanged(_) | Op::ThreadClosed(_) => {
                    self.passive.gap = true;
                    applied()
                }
            },
        }
    }

    pub(in super::super) fn passive_completion(
        &mut self,
        reply: BrokerReply,
    ) -> (BrokerReply, bool) {
        match reply {
            BrokerReply::SteeringMode(Ok(_)) => (
                BrokerReply::SteeringMode(Ok(SteeringUserMessageCaptureMode::Passive)),
                false,
            ),
            reply @ (BrokerReply::SteeringSelected(_)
            | BrokerReply::SteeringChecked
            | BrokerReply::SteeringAbandoned) => (reply, false),
            BrokerReply::SteeringCheckedRejected(message, _) => {
                self.passive_reply(BrokerOperation::SteeringChecked(message))
            }
            BrokerReply::Rejected(OrderedTurnStreamOperation::Approval(request), _)
            | BrokerReply::Applied(OrderedTurnStreamCompletion::Approval(
                ApprovalOperationCompletion::TargetFailed { request, .. },
            )) => self.passive_reply(BrokerOperation::Ordered(
                OrderedTurnStreamOperation::Approval(request),
            )),
            BrokerReply::Rejected(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict),
            ) => {
                // The failed operation has already been observed; do not replay its content.
                match operation {
                    OrderedTurnStreamOperation::ProviderFragment(fragment) => {
                        let mut page = fragment.into_lease();
                        page.clear();
                        (
                            BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                            false,
                        )
                    }
                    OrderedTurnStreamOperation::Approval(request) => self.passive_reply(
                        BrokerOperation::Ordered(OrderedTurnStreamOperation::Approval(request)),
                    ),
                    _ => (
                        BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                        false,
                    ),
                }
            }
            BrokerReply::SteeringSelectionRejected(
                selection,
                cause @ (OrderedTurnStreamSubmitCause::Unavailable
                | OrderedTurnStreamSubmitCause::Cancelled
                | OrderedTurnStreamSubmitCause::Rejected(
                    OrderedTurnStreamRejection::StagingConflict,
                )),
            ) => (
                BrokerReply::SteeringSelectionRejected(selection, cause),
                false,
            ),
            BrokerReply::Applied(completion) => (BrokerReply::Applied(completion), false),
            other => (other, true),
        }
    }
}
