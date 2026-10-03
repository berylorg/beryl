use super::*;
use crate::main_window::{MainWindowFailedResidentCapture, MainWindowFailedResidentPreparation};

impl RunningProcessOwner {
    pub(super) fn prepare_failed_interrupted_exit_resident(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        resident: &Entity<MainWindowConversationComposer>,
        close: MainWindowConversationComposerCloseTicket,
        window: AnyWindowHandle,
        generation: beryl_home_store::HomeGeneration,
        capture: MainWindowFailedResidentCapture,
        environment: Environment,
        app: &mut App,
        completed: Completion,
    ) -> Result<ResidentPreparationKey, (MainWindowFailedResidentCapture, String)> {
        let mut retained = owner.borrow_mut();
        let home = capture.selection().binding().home_id();
        let validation = (|| -> Result<(), String> {
            retained.interrupted_exit_services_result(request)?;
            let recovery = retained
                .interrupted_exit
                .as_ref()
                .ok_or("No reported failed Exit")?;
            if !recovery
                .residents
                .contains(&(window, resident.entity_id(), close))
                || recovery.resident.is_some()
                || recovery.session.borrow().is_none()
                || recovery
                    .pending_resident_frame
                    .as_ref()
                    .is_some_and(|wake| wake.strong_count() != 0)
            {
                return Err("failed resident request or flight custody changed".into());
            }
            resident.read(app).validate_failed_capture(&capture, app)?;
            let mut slot = recovery.settlement.borrow_mut();
            let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
                return Err("failed resident candidate graph is unavailable".into());
            };
            if !graph.matches_candidate(home, generation)
                || generation == capture.selection().binding().home_generation()
            {
                return Err("failed resident candidate generation changed".into());
            }
            Ok(())
        })();
        if let Err(error) = validation {
            return Err((capture, error));
        }
        let recovery = retained.interrupted_exit.as_mut().unwrap();
        let Some(CandidateSettlement::Services(Ok(graph))) = recovery
            .settlement
            .borrow_mut()
            .replace(CandidateSettlement::Pending)
        else {
            unreachable!()
        };
        let key = ResidentPreparationKey(Rc::new(()));
        let weak_owner = Rc::downgrade(owner);
        let closed_key = key.clone();
        let window_closed = app.on_window_closed(move |app| {
            if !app.windows().contains(&window) {
                if let Some(owner) = weak_owner.upgrade() {
                    let _ = Self::cancel_interrupted_exit_resident(&owner, &closed_key, app);
                }
            }
        });
        let wake_owner = owner.clone();
        let wake_key = key.clone();
        let preparation =
            MainWindowFailedResidentPreparation::prepare_graph(graph, capture, app, move |app| {
                Self::schedule_resident_preparation(&wake_owner, &wake_key, app)
            });
        recovery.resident = Some(ResidentPreparation {
            key: key.clone(),
            request: request.identity(),
            resident: resident.downgrade(),
            close,
            home,
            generation,
            window,
            preparation: Preparation::Failed(Box::new(preparation)),
            environment: Some(environment),
            result: Ok(Progress::Waiting),
            cancelled: false,
            cleanup_failed: false,
            scheduled: None,
            returned: None,
            completed: Some(completed),
            _window_closed: window_closed,
        });
        Ok(key)
    }
}
