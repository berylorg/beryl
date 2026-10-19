use super::*;

impl ActivityViewport {
    pub(super) fn prepare_step(&mut self, cancelled: &AtomicBool) -> ActivityViewportStep {
        let mut preparing = self.preparing.take().unwrap();
        if preparing.rows.len() == preparing.page.records().len() {
            let end = preparing.start.saturating_add(preparing.rows.len() as u64);
            let next = preparing.page.next();
            let facts = match self
                .selection
                .observe(self.collection.as_ref().map(ActivityCollection::head))
            {
                Ok(facts) => facts,
                Err(error) => {
                    self.preparing = Some(preparing);
                    return self.read_failure(error);
                }
            };
            if self.execution.as_ref() != Some(&facts.execution) {
                self.replace();
                return ActivityViewportStep::Pending;
            }
            self.facts = Some(facts);
            self.advance(end, next);
            self.resident.push_back(Arc::new(ActivityPreparedPage {
                start: preparing.start,
                rows: preparing.rows,
            }));
            while self.resident.len() > RESIDENT_PAGES {
                self.resident.pop_front();
            }
            if self.covered() {
                self.ready = true;
                self.failure = None;
                self.retrying = false;
                return ActivityViewportStep::Ready;
            }
            if self.resident.len() == RESIDENT_PAGES || self.walk.cursor.is_none() {
                return self.fail("Activity viewport exceeds the resident page capacity");
            }
            return ActivityViewportStep::Pending;
        }
        let entry = &preparing.page.records()[preparing.rows.len()];
        let metadata = self
            .metadata
            .iter()
            .find(|metadata| metadata.thread_id() == entry.source().cas_item().turn().thread_id());
        if let Some(row) = display::format_handoff(self.selection.thread_id(), entry, metadata) {
            preparing.rows.push(row);
            self.preparing = Some(preparing);
            return ActivityViewportStep::Pending;
        }
        let operation = match self.selection.begin() {
            Ok(operation) => operation,
            Err(error) => {
                self.preparing = Some(preparing);
                return self.read_failure(error);
            }
        };
        let resources = operation.resources();
        let cancellation = || {
            cancelled.load(Ordering::Acquire) || !self.selection.current() || !self.demand.current()
        };
        if preparing.extraction.is_none() {
            match resources
                .storage
                .prepare_activity_display(&resources.home, entry)
            {
                Ok(extraction) => preparing.extraction = Some(extraction),
                Err(error) => {
                    self.preparing = Some(preparing);
                    return self.extraction_failure(error);
                }
            }
            self.preparing = Some(preparing);
            return ActivityViewportStep::Pending;
        }
        match preparing.extraction.as_mut().unwrap().step(
            &resources.home,
            &resources.storage,
            &cancellation,
        ) {
            Ok(ActivityDisplayStep::Pending) => {}
            Ok(ActivityDisplayStep::Complete(source)) => {
                preparing.rows.push(display::format_row(
                    self.selection.thread_id(),
                    self.execution.as_ref(),
                    entry,
                    source,
                    metadata,
                ));
                preparing.extraction = None;
            }
            Err(error) => {
                preparing.extraction = None;
                self.preparing = Some(preparing);
                return self.extraction_failure(error);
            }
        }
        self.preparing = Some(preparing);
        ActivityViewportStep::Pending
    }
}
