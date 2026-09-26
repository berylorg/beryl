use super::*;
use crate::cas_projection::runtime_work::RuntimeWorkError;

impl ContextCompactionCoordinator {
    pub(in crate::cas_projection) fn verify_control_guard_exclusion_for_test(&self) {
        let stop_revision = self.stop.try_work_revision().unwrap();
        let revision = self.try_work_revision().unwrap();
        let operations = self.operations.lock().unwrap();
        assert_eq!(
            self.try_hold_control_revisions(&self.stop, stop_revision, revision)
                .err(),
            Some(RuntimeWorkError::Busy)
        );
        assert_eq!(self.stop.try_work_revision(), Ok(stop_revision));
        drop(operations);
        let work = self.custody.source.try_hold_revision(revision).unwrap();
        assert_eq!(
            self.try_hold_control_revisions(&self.stop, stop_revision, revision)
                .err(),
            Some(RuntimeWorkError::Busy)
        );
        assert_eq!(self.stop.try_work_revision(), Ok(stop_revision));
        drop(self.operations.try_lock().unwrap());
        drop(work);
        let retained = self
            .try_hold_control_revisions(&self.stop, stop_revision, revision)
            .unwrap();
        assert!(matches!(
            self.operations.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        assert_eq!(
            self.custody.source.try_revision(),
            Err(RuntimeWorkError::Busy)
        );
        drop(retained);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _operations = self.operations.lock().unwrap();
                panic!("inject operations-only poison");
            }))
            .is_err()
        );
        assert_eq!(
            self.try_hold_control_revisions(&self.stop, stop_revision, revision)
                .err(),
            Some(RuntimeWorkError::Unavailable)
        );
        assert_eq!(self.stop.try_work_revision(), Ok(stop_revision));
        assert_eq!(self.custody.source.try_revision(), Ok(revision));
    }
}
