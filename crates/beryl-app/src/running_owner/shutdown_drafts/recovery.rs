use super::*;

impl RunningShutdownDrafts {
    pub(in crate::running_owner) fn retire_residents(
        &mut self,
        app: &mut App,
    ) -> Result<bool, String> {
        if !self.ready() || self.released {
            return Err("Interrupted Exit draft set is not ready".into());
        }
        let mut ready = true;
        let mut failure = None;
        for (window, draft) in &mut self.windows {
            let result = match draft {
                Ok(draft) => window
                    .update(app, |root, _, cx| root.retire_shutdown_draft(draft, cx))
                    .map_err(|error| format!("Interrupted Exit window is unavailable: {error}"))
                    .and_then(|result| result),
                Err(error) => Err(error.clone()),
            };
            match result {
                Ok(retired) => ready &= retired,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(ready),
        }
    }
}
