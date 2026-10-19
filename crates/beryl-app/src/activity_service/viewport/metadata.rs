use super::*;
use beryl_backend::ThreadReadMetadata;
use beryl_model::CasThreadId;

pub(crate) struct ActivityMetadataRequest {
    expected: ActivityPreparedWindow,
    thread: CasThreadId,
}

pub(crate) struct ActivityMetadataCompletion {
    expected: ActivityPreparedWindow,
    metadata: display::ActivityAgentMetadata,
}

impl ActivityPreparedWindow {
    pub(crate) fn metadata_request(&self, thread: &CasThreadId) -> Option<ActivityMetadataRequest> {
        self.with_current(|_| {
            self.pages
                .iter()
                .flat_map(|page| &page.rows)
                .any(|row| row.source().cas_item().turn().thread_id() == thread)
                .then(|| ActivityMetadataRequest {
                    expected: self.clone(),
                    thread: thread.clone(),
                })
        })
        .ok()
        .flatten()
    }
}

impl ActivityMetadataRequest {
    pub(crate) fn complete(
        &self,
        metadata: &ThreadReadMetadata,
    ) -> Option<ActivityMetadataCompletion> {
        if metadata.thread_id() != &self.thread {
            return None;
        }
        let value = display::ActivityAgentMetadata::new(
            self.thread.clone(),
            metadata.agent_nickname()?,
            None,
            None,
        )?;
        self.expected.with_current(|_| ()).ok()?;
        Some(ActivityMetadataCompletion {
            expected: self.expected.clone(),
            metadata: value,
        })
    }
}

impl ActivityViewport {
    pub(crate) fn set_metadata(&mut self, completion: &ActivityMetadataCompletion) -> bool {
        let ActivityMetadataCompletion { expected, metadata } = completion;
        if !self
            .collection
            .as_ref()
            .is_some_and(|collection| collection.same_query(&expected.collection))
            || !self.demand.current()
            || !self
                .resident
                .iter()
                .flat_map(|page| &page.rows)
                .any(|row| row.source().cas_item().turn().thread_id() == metadata.thread_id())
        {
            return false;
        }
        let mut pages = VecDeque::new();
        for page in &self.resident {
            let mut rows = page.rows.clone();
            for row in &mut rows {
                if row.source().cas_item().turn().thread_id() == metadata.thread_id() {
                    row.decorate_agent(self.selection.thread_id(), Some(&metadata));
                }
            }
            pages.push_back(Arc::new(ActivityPreparedPage {
                start: page.start,
                rows,
            }));
        }
        expected
            .with_current(|_| {
                self.active.store(false, Ordering::Release);
                self.active = Arc::new(AtomicBool::new(true));
                self.resident = pages;
                self.metadata
                    .retain(|prior| prior.thread_id() != metadata.thread_id());
                self.metadata.push_back(metadata.clone());
                while self.metadata.len() > 256 {
                    self.metadata.pop_front();
                }
            })
            .is_ok()
    }
}
