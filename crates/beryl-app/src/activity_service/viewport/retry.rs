use super::*;

#[derive(Clone)]
pub(crate) struct ActivityRetry(Weak<()>);

impl ActivityRetry {
    pub(crate) fn same_request(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl ActivityViewport {
    pub(crate) fn retry_request(&self) -> Option<ActivityRetry> {
        self.failure_token
            .as_ref()
            .map(|token| ActivityRetry(Arc::downgrade(token)))
    }

    pub(crate) fn retry_exact(&mut self, request: &ActivityRetry) -> bool {
        if !self
            .failure_token
            .as_ref()
            .is_some_and(|token| request.0.ptr_eq(&Arc::downgrade(token)))
        {
            return false;
        }
        self.retry()
    }
}
