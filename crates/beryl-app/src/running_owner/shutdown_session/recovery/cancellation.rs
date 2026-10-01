use super::*;
use beryl_home_store::HomeGeneration;
use gpui::AsyncApp;
use settlement::CandidateSettlement;

enum CancelledPreparation {
    Constructed,
    Settled,
    Services,
}

impl RunningProcessOwner {
    pub(super) async fn dispose_cancelled_interrupted_exit_preparation(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let stage = {
            let owner = owner.borrow();
            owner.interrupted_exit_graph_retirement_result(request)?;
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            match recovery.settlement.borrow().as_ref() {
                Some(CandidateSettlement::Constructed(Ok(_))) => CancelledPreparation::Constructed,
                Some(CandidateSettlement::Returned { .. }) => CancelledPreparation::Settled,
                Some(CandidateSettlement::Services(Ok(_))) => CancelledPreparation::Services,
                _ => return Ok(()),
            }
        };
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let completed = move |_: &Rc<RefCell<Self>>, _: &mut App| {
                let _ = sender.send(());
            };
            match stage {
                CancelledPreparation::Constructed => Self::abort_constructed_exit_candidate(
                    owner, request, generation, app, completed,
                ),
                CancelledPreparation::Settled => Self::dispose_settled_interrupted_exit_candidate(
                    owner, request, generation, true, app, completed,
                ),
                CancelledPreparation::Services => {
                    Self::cancel_interrupted_exit_services(owner, request, app, completed)
                }
            }
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit cancellation disposal delivery is unavailable")?;
        if matches!(stage, CancelledPreparation::Services) {
            owner
                .borrow_mut()
                .return_interrupted_exit_preparation_home(request, generation)?;
        }
        Ok(())
    }
}
