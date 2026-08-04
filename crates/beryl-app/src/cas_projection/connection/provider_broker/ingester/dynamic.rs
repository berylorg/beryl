use beryl_backend::{
    DynamicToolArgumentControl, DynamicToolArgumentFragment, DynamicToolCall,
    DynamicToolCallAbandonReason, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause,
};

use super::{ActiveDynamicTool, ActiveIngress, BrokerReply, Ingester};
use crate::{
    cas_projection::connection::router::DynamicToolTargetError,
    conversation_tools::InstalledArgumentBuilder,
};

impl Ingester {
    pub(super) fn begin_dynamic(&mut self, call: DynamicToolCall) -> (BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::DynamicBegin(call),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let permit = match self.router.reserve_dynamic_tool(self.live_command(), &call) {
            Ok(permit) => permit,
            Err(error) => {
                self.record_dynamic_target_error(error);
                return self.reject_dynamic_cause(
                    OrderedTurnStreamOperation::DynamicBegin(call),
                    OrderedTurnStreamSubmitCause::Unavailable,
                );
            }
        };
        let builder = InstalledArgumentBuilder::select(call.namespace(), call.tool());
        self.put_dynamic(ActiveDynamicTool {
            call,
            builder,
            permit,
        });
        (
            BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            false,
        )
    }

    pub(super) fn control_dynamic(
        &mut self,
        control: DynamicToolArgumentControl,
    ) -> (BrokerReply, bool) {
        if !self.dynamic_is_live() {
            return self.reject(
                OrderedTurnStreamOperation::DynamicArgumentControl(control),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let Some(ActiveIngress::Dynamic(dynamic)) = self.active.as_mut() else {
            unreachable!("live dynamic-tool ingress retains its active builder")
        };
        dynamic.builder.control(control);
        (
            BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            false,
        )
    }

    pub(super) fn acquire_dynamic_page(&mut self) -> (BrokerReply, bool) {
        if !self.dynamic_is_live() {
            return self.reject(
                OrderedTurnStreamOperation::DynamicAcquirePage,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        match self.pages.try_lease() {
            Ok(page) => (
                BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                false,
            ),
            Err(_) => self.reject_dynamic_cause(
                OrderedTurnStreamOperation::DynamicAcquirePage,
                OrderedTurnStreamSubmitCause::CapacityFull,
            ),
        }
    }

    pub(super) fn fragment_dynamic(
        &mut self,
        fragment: DynamicToolArgumentFragment,
    ) -> (BrokerReply, bool) {
        if !self.dynamic_is_live() {
            return self.reject(
                OrderedTurnStreamOperation::DynamicArgumentFragment(fragment),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let Some(ActiveIngress::Dynamic(dynamic)) = self.active.as_mut() else {
            unreachable!("live dynamic-tool ingress retains its active builder")
        };
        dynamic
            .builder
            .fragment(fragment.kind(), fragment.offset(), fragment.bytes());
        let mut page = fragment.into_lease();
        page.clear();
        (
            BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
            false,
        )
    }

    pub(super) fn seal_dynamic(&mut self) -> (BrokerReply, bool) {
        if !self.dynamic_is_live() {
            return self.reject(
                OrderedTurnStreamOperation::DynamicSeal,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let dynamic = self
            .take_dynamic()
            .expect("live dynamic-tool seal retains its sole active builder");
        let request = dynamic.builder.seal();
        match self.router.seal_dynamic_tool(
            self.live_command(),
            dynamic.permit,
            dynamic.call,
            request,
        ) {
            Ok(()) => (
                BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                false,
            ),
            Err(error) => {
                self.record_dynamic_target_error(error);
                self.reject_dynamic_cause(
                    OrderedTurnStreamOperation::DynamicSeal,
                    OrderedTurnStreamSubmitCause::Unavailable,
                )
            }
        }
    }

    pub(super) fn abandon_dynamic(
        &mut self,
        _reason: DynamicToolCallAbandonReason,
    ) -> (BrokerReply, bool) {
        let Some(dynamic) = self.take_dynamic() else {
            return self.reject(
                OrderedTurnStreamOperation::DynamicAbandon(_reason),
                OrderedTurnStreamRejection::InvalidControl,
            );
        };
        self.router.abandon_dynamic_tool(&dynamic.permit);
        drop(dynamic);
        (
            BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            false,
        )
    }

    fn dynamic_is_live(&self) -> bool {
        let Some(ActiveIngress::Dynamic(dynamic)) = self.active.as_ref() else {
            return false;
        };
        self.router
            .dynamic_tool_is_live(self.live_command(), &dynamic.permit)
    }

    fn record_dynamic_target_error(&self, error: DynamicToolTargetError) {
        match error {
            DynamicToolTargetError::Target(target) => {
                let _ = self.invalidate_target(target);
            }
            DynamicToolTargetError::Unmatched | DynamicToolTargetError::Router => {}
        }
    }

    fn reject_dynamic_cause(
        &mut self,
        operation: OrderedTurnStreamOperation,
        cause: OrderedTurnStreamSubmitCause,
    ) -> (BrokerReply, bool) {
        self.abandon_active();
        self.retire();
        (BrokerReply::Rejected(operation, cause), true)
    }
}
