use super::*;
use crate::main_window::{MainWindowFreshClaimWidgetBatch, MainWindowFreshClaimWidgetReply};

impl PreparedRecoveryServiceGraph {
    pub(crate) fn prepare_claim_widget_batch(
        &mut self,
        window: beryl_model::WindowId,
        batch: Arc<MainWindowFreshClaimWidgetBatch>,
        cancellation: &CommandCancellation,
    ) -> Result<(), String> {
        if !self.has_committed_claim(window) || batch.window_id() != window {
            return Err("fresh ordinary widget batch has no authenticated committed owner".into());
        }
        let original = self
            .failed_claims
            .iter()
            .find(|source| source.window == window)
            .unwrap();
        if !matches!(
            original.operation.as_ref(),
            RetiredClaimOperation::Ordinary(_)
        ) {
            return Err("creation permission cannot prepare an ordinary widget batch".into());
        }
        let prepared = self
            .claim_targets
            .iter_mut()
            .find(|target| target.window == window)
            .ok_or("fresh ordinary widget target is missing")?;
        if let Some(reply) = &prepared.widget_reply {
            if !Arc::ptr_eq(reply.batch(), &batch) {
                return Err("fresh ordinary widget already retains a different batch".into());
            }
        } else {
            prepared.widget_reply = Some(MainWindowFreshClaimWidgetReply::pending(batch));
        }
        let (candidate, _) = self
            .services
            .as_mut()
            .unwrap()
            .cas
            .as_mut()
            .unwrap()
            .app_preparation_parts()
            .ok_or("fresh ordinary widget candidate is unavailable")?;
        prepared.composer.prepare_widget_batch(
            candidate,
            prepared.widget_reply.as_mut().unwrap(),
            cancellation,
        )
    }

    pub(crate) fn claim_widget_reply(
        &mut self,
        window: beryl_model::WindowId,
    ) -> Result<&mut Option<Box<MainWindowFreshClaimWidgetReply>>, String> {
        Ok(&mut self
            .claim_targets
            .iter_mut()
            .find(|target| target.window == window)
            .ok_or("fresh ordinary reply target is missing")?
            .widget_reply)
    }
}
