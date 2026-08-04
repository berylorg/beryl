use std::time::Duration;

use beryl_backend::{
    NormalTurnTerminalStatus, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamSubmitError, lifecycle_test_support::normal_turn_terminal,
};
use beryl_home_store::test_faults::FaultBlock;
use beryl_model::{CasThreadId, CasTurnId};

use super::CheckedUserFixture;
use crate::cas_projection::connection::router::LiveEventTargetCloseReason;

struct FaultBlockRelease<'a> {
    block: &'a FaultBlock,
    armed: bool,
}

impl<'a> FaultBlockRelease<'a> {
    const fn new(block: &'a FaultBlock) -> Self {
        Self { block, armed: true }
    }

    fn release(mut self) {
        self.block.release();
        self.armed = false;
    }
}

impl Drop for FaultBlockRelease<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.block.release();
        }
    }
}

impl CheckedUserFixture {
    pub(in super::super) fn submit_terminal(&mut self, status: NormalTurnTerminalStatus) {
        let result = self.try_submit_terminal_for_route(
            status,
            self.cas_thread_id.clone(),
            self.cas_turn_id.clone(),
        );
        assert!(matches!(result, Ok(OrderedTurnStreamCompletion::Applied)));
    }

    pub(in super::super) fn try_submit_terminal_for_route(
        &mut self,
        status: NormalTurnTerminalStatus,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let terminal = normal_turn_terminal(status, thread_id, turn_id);
        self.sink
            .as_mut()
            .expect("checked-user sink remains open")
            .submit(OrderedTurnStreamOperation::NormalTurnTerminal(terminal))
    }

    pub(in super::super) fn submit_terminal_while_publication_paused<R>(
        &mut self,
        status: NormalTurnTerminalStatus,
        block: &FaultBlock,
        inspect: impl FnOnce(&Self) -> R,
    ) -> (bool, R) {
        let terminal =
            normal_turn_terminal(status, self.cas_thread_id.clone(), self.cas_turn_id.clone());
        let mut sink = self.sink.take().expect("checked-user sink remains open");
        let (returned_sink, result, reached, inspection) = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let result = sink.submit(OrderedTurnStreamOperation::NormalTurnTerminal(terminal));
                (sink, result)
            });
            let reached = block.wait_until_reached(Duration::from_secs(1));
            let release = FaultBlockRelease::new(block);
            let inspection = inspect(self);
            release.release();
            let (sink, result) = worker.join().unwrap();
            (sink, result, reached, inspection)
        });
        self.sink = Some(returned_sink);
        assert!(matches!(result, Ok(OrderedTurnStreamCompletion::Applied)));
        (reached, inspection)
    }

    pub(in super::super) fn request_target_close(
        &self,
        reason: LiveEventTargetCloseReason,
    ) -> bool {
        self.router.unregister(&self.registration, reason)
    }

    pub(in super::super) fn router_target_count(&self) -> usize {
        self.router.snapshot().unwrap().target_count()
    }
}
