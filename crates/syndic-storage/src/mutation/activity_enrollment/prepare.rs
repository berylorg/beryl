use super::*;

impl SyndicStorage {
    pub fn prepare_activity_enrollment(
        &self,
        store: &HomeStore,
        request: ActivityEnrollmentRequest,
    ) -> Result<ActivityEnrollmentPreparation, SyndicReadError> {
        let expected_home = store.home_revision()?;
        let domain_revision = self.revision(store)?;
        let thread_id = request.source.thread_id();
        let thread = self
            .point::<ThreadsFamily>(store, thread_id, limit())?
            .ok_or_else(invalid)?;
        let execution = self
            .point::<ThreadExecutionsFamily>(store, thread_id, limit())?
            .ok_or_else(invalid)?;
        let turn = self
            .point::<TurnsFamily>(store, request.source.turn_id(), limit())?
            .ok_or_else(invalid)?;
        let state = self
            .point::<TurnStatesFamily>(store, request.source.turn_id(), limit())?
            .ok_or_else(invalid)?;
        let old_head = self
            .point::<ActivityQueryHeadsFamily>(store, thread_id, limit())?
            .ok_or_else(invalid)?;
        if execution.execution() != &request.execution
            || thread.id() != thread_id
            || turn.id() != request.source.turn_id()
            || state.turn_id() != request.source.turn_id()
            || execution.thread_id() != thread_id
            || turn.origin_thread_id() != thread_id
            || state.turn_id() != turn.id()
            || thread.committed_tail() != Some(turn.id())
            || state.lifecycle().is_proven_terminal()
            || old_head.thread_id() != thread_id
            || old_head.revision() != request.head_revision
            || old_head.work_period().get() > expected_home.get()
        {
            return Err(invalid());
        }
        let token = match request.token {
            Some(token) => {
                if token.home != store.home_id()
                    || token.runtime != request.execution.runtime_id()
                    || token.period.get() > expected_home.get()
                    || token.period < old_head.work_period()
                {
                    return Err(invalid());
                }
                token
            }
            None => ActivityPeriodToken {
                home: store.home_id(),
                runtime: request.execution.runtime_id(),
                period: allocate_period(expected_home)?,
            },
        };
        let same_period = old_head.work_period() == token.period;
        let next_member = ActivityQuerySourceRecord::new(
            thread_id,
            token.period,
            request.source,
            None,
            0,
            true,
            None,
        );
        if same_period && old_head.source() == Some(request.source) {
            let member = self
                .point::<ActivityQuerySourcesFamily>(store, source_key(&next_member), limit())?
                .ok_or_else(invalid)?;
            if !old_head.source_active()
                || old_head.lifecycle() != ProjectionLifecycle::Current
                || member.source() != request.source
                || member.thread_id() != thread_id
                || member.work_period() != token.period
                || !member.active()
                || member.source_frontier() != state.source_event_count()
                || member.child_handoff().is_some()
            {
                return Err(invalid());
            }
            unchanged(store, expected_home)?;
            return Ok(ActivityEnrollmentPreparation::AlreadyEnrolled(token));
        }
        if old_head.source_active()
            || old_head.running_row_count() != 0
            || state.source_event_count() != 0
            || (same_period && old_head.lifecycle() != ProjectionLifecycle::Current)
        {
            return Err(invalid());
        }
        let (previous_state, previous_member) = match old_head.source() {
            Some(source) => {
                let previous_turn = self
                    .point::<TurnsFamily>(store, source.turn_id(), limit())?
                    .ok_or_else(invalid)?;
                let state = self
                    .point::<TurnStatesFamily>(store, source.turn_id(), limit())?
                    .ok_or_else(invalid)?;
                let member = self
                    .point::<ActivityQuerySourcesFamily>(
                        store,
                        ActivityQuerySourceKey {
                            thread: thread_id,
                            work_period: old_head.work_period(),
                            source_thread: source.thread_id(),
                            source_turn: source.turn_id(),
                        },
                        limit(),
                    )?
                    .ok_or_else(invalid)?;
                let stale = !same_period && old_head.lifecycle() == ProjectionLifecycle::Stale;
                if source.thread_id() != thread_id
                    || previous_turn.id() != source.turn_id()
                    || previous_turn.origin_thread_id() != thread_id
                    || state.turn_id() != source.turn_id()
                    || !state.lifecycle().is_proven_terminal()
                    || (!stale && member.active())
                    || member.source() != source
                    || member.source_frontier() > state.source_event_count()
                    || (!stale && member.source_frontier() != state.source_event_count())
                    || member.thread_id() != thread_id
                    || member.work_period() != old_head.work_period()
                    || member.child_handoff().is_some()
                {
                    return Err(invalid());
                }
                (Some(state), Some(member))
            }
            None => (None, None),
        };
        let mut deleted = Vec::new();
        if same_period {
            let page = store.read_cursor::<SyndicDomain, ActivityQueryEntriesCodec>(
                &self.handle,
                &running_range(&old_head),
                CursorDirection::Forward,
                page_limits(ACTIVITY_ENROLLMENT_CLEANUP_ROWS),
            )?;
            for record in page
                .records()
                .iter()
                .take_while(|record| record.key().order.running())
            {
                let key = *record.key();
                let value = record.value();
                let source = value.source();
                if key.thread != thread_id
                    || key.work_period != token.period
                    || value.thread_id() != thread_id
                    || value.work_period() != token.period
                    || value.order() != key.order
                {
                    return Err(invalid());
                }
                let source_state = self
                    .point::<TurnStatesFamily>(store, source.turn_id(), limit())?
                    .ok_or_else(invalid)?;
                let source_turn = self
                    .point::<TurnsFamily>(store, source.turn_id(), limit())?
                    .ok_or_else(invalid)?;
                let member = self
                    .point::<ActivityQuerySourcesFamily>(
                        store,
                        ActivityQuerySourceKey {
                            thread: thread_id,
                            work_period: token.period,
                            source_thread: source.thread_id(),
                            source_turn: source.turn_id(),
                        },
                        limit(),
                    )?
                    .ok_or_else(invalid)?;
                if source_state.turn_id() != source.turn_id()
                    || !source_state.lifecycle().is_proven_terminal()
                    || source_turn.id() != source.turn_id()
                    || source_turn.origin_thread_id() != source.thread_id()
                    || member.thread_id() != thread_id
                    || member.work_period() != token.period
                    || member.source()
                        != ActivityQuerySource::new(source.thread_id(), source.turn_id())
                    || member.active()
                    || member.source_frontier() != source_state.source_event_count()
                    || member.child_handoff().is_some()
                    || value.source_event().get() > member.source_frontier()
                {
                    return Err(invalid());
                }
                deleted.push((key, value.clone()));
            }
        } else {
            let entries = store.read_cursor::<SyndicDomain, ActivityQueryEntriesCodec>(
                &self.handle,
                &period_entries(thread_id, token.period),
                CursorDirection::Forward,
                page_limits(1),
            )?;
            let sources = store.read_cursor::<SyndicDomain, ActivityQuerySourcesCodec>(
                &self.handle,
                &period_sources(thread_id, token.period),
                CursorDirection::Forward,
                page_limits(1),
            )?;
            if !entries.records().is_empty() || !sources.records().is_empty() {
                return Err(invalid());
            }
        }
        let cleanup = !deleted.is_empty();
        let revision = old_head.revision().checked_next().map_err(|_| invalid())?;
        let source = (!cleanup).then_some(next_member);
        if let Some(source) = &source {
            if self
                .point::<ActivityQuerySourcesFamily>(store, source_key(source), limit())?
                .is_some()
            {
                return Err(invalid());
            }
        }
        let new_head = if cleanup {
            ActivityQueryHeadRecord::new(
                thread_id,
                old_head.work_period(),
                old_head.source(),
                false,
                old_head.source_frontier(),
                revision,
                old_head.source_count(),
                old_head.logical_row_count(),
                0,
                old_head.completed_row_count(),
                old_head.completed_stored_bytes(),
                old_head.completed_retention_cutoff(),
                old_head.lifecycle(),
            )
        } else if same_period {
            ActivityQueryHeadRecord::new(
                thread_id,
                token.period,
                Some(request.source),
                true,
                old_head.source_frontier(),
                revision,
                old_head.source_count().checked_add(1).ok_or_else(invalid)?,
                old_head.logical_row_count(),
                0,
                old_head.completed_row_count(),
                old_head.completed_stored_bytes(),
                old_head.completed_retention_cutoff(),
                ProjectionLifecycle::Current,
            )
        } else {
            ActivityQueryHeadRecord::new(
                thread_id,
                token.period,
                Some(request.source),
                true,
                0,
                revision,
                1,
                0,
                0,
                0,
                0,
                None,
                ProjectionLifecycle::Current,
            )
        }
        .map_err(|_| invalid())?;
        unchanged(store, expected_home)?;
        Ok(ActivityEnrollmentPreparation::Prepared(
            PreparedActivityEnrollment {
                handle: self.handle.clone(),
                domain_revision,
                expected_home,
                records: Arc::new(EnrollmentRecords {
                    home: store.home_id(),
                    token,
                    old_head,
                    new_head,
                    source,
                    deleted,
                    thread,
                    execution,
                    turn,
                    state,
                    previous_state,
                    previous_member,
                }),
            },
        ))
    }
}

fn unchanged(store: &HomeStore, revision: HomeRevision) -> Result<(), SyndicReadError> {
    if store.home_revision()? != revision {
        return Err(SyndicReadError::ConcurrentChange {
            operation: "activity enrollment",
        });
    }
    Ok(())
}

fn allocate_period(revision: HomeRevision) -> Result<ActivityWorkPeriod, SyndicReadError> {
    ActivityWorkPeriod::new(revision.checked_next().map_err(|_| invalid())?.get())
        .map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/activity_enrollment.rs"
    ));
}
