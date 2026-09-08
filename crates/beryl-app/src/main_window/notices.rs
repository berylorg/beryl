mod model;
mod widget;

use beryl_model::WindowId;
use model::Identity;

pub use model::*;
pub use widget::*;

struct Entry {
    token: NoticeRecordToken,
    kind: NoticeKind,
    content: NoticeContent,
    report_count: u64,
}

pub struct MainWindowNoticeArbiter {
    window_id: WindowId,
    owner: Identity,
    entries: Vec<Entry>,
    visible: Option<(Identity, Identity)>,
    diagnostics: NoticeDiagnostics,
}

impl MainWindowNoticeArbiter {
    pub fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            owner: Identity::new(),
            entries: Vec::with_capacity(NOTICE_RECORD_CAPACITY),
            visible: None,
            diagnostics: NoticeDiagnostics::default(),
        }
    }

    pub fn active(&self) -> Option<NoticeProjection<'_>> {
        let (admission, exposure) = self.visible.as_ref()?;
        let entry = self
            .entries
            .iter()
            .find(|entry| &entry.token.admission == admission)?;
        Some(NoticeProjection {
            token: NoticeVisibleToken {
                record: entry.token.clone(),
                exposure: exposure.clone(),
            },
            kind: entry.kind,
            content: &entry.content,
            report_count: entry.report_count,
        })
    }

    pub fn admit(&mut self, record: NoticeRecord) -> NoticeAdmission {
        if let Err(reason) = self.validate_record(&record) {
            return self.reject_admission(reason);
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.token.condition == record.condition)
        {
            return match self.update_entry(index, record) {
                Ok(token) => NoticeAdmission::Updated(token),
                Err(reason) => self.reject_admission(reason),
            };
        }
        if record.kind.protected() {
            if self.entries.iter().any(|entry| entry.kind == record.kind) {
                return self.reject_admission(NoticeRejection::ProtectedConditionOccupied);
            }
        } else if self
            .entries
            .iter()
            .filter(|entry| !entry.kind.protected())
            .count()
            == NOTICE_GENERAL_CAPACITY
        {
            let replacement = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    !entry.kind.protected() && entry.kind > record.kind && !self.is_active(entry)
                })
                .max_by_key(|(index, entry)| (entry.kind, *index))
                .map(|(index, _)| index);
            let Some(index) = replacement else {
                self.diagnostics.omitted = self.diagnostics.omitted.saturating_add(1);
                return NoticeAdmission::Omitted;
            };
            self.entries.remove(index);
            self.diagnostics.replaced = self.diagnostics.replaced.saturating_add(1);
        }
        let token = self.insert(record);
        self.refresh_visible();
        NoticeAdmission::Admitted(token)
    }

    pub fn update(
        &mut self,
        expected: &NoticeRecordToken,
        revision: u64,
        content: NoticeContent,
    ) -> Result<NoticeRecordToken, NoticeRejection> {
        let result = self.find_exact(expected).and_then(|index| {
            self.update_entry(
                index,
                NoticeRecord {
                    window_id: self.window_id,
                    condition: expected.condition.clone(),
                    revision,
                    kind: self.entries[index].kind,
                    content,
                },
            )
        });
        self.count_rejection(result)
    }

    pub fn replace_protected(
        &mut self,
        expected: &NoticeRecordToken,
        replacement: NoticeRecord,
    ) -> Result<NoticeRecordToken, NoticeRejection> {
        let result = (|| {
            self.validate_record(&replacement)?;
            let index = self.find_exact(expected)?;
            if !replacement.kind.protected()
                || self.entries[index].kind != replacement.kind
                || replacement.condition == expected.condition
                || self
                    .entries
                    .iter()
                    .any(|entry| entry.token.condition == replacement.condition)
            {
                return Err(NoticeRejection::InvalidProtectedReplacement);
            }
            self.entries.remove(index);
            self.diagnostics.replaced = self.diagnostics.replaced.saturating_add(1);
            let token = self.insert(replacement);
            self.refresh_visible();
            Ok(token)
        })();
        self.count_rejection(result)
    }

    pub fn dismiss(&mut self, expected: &NoticeVisibleToken) -> Result<(), NoticeRejection> {
        let result = (|| {
            let index = self.find_exact(&expected.record)?;
            if self.visible.as_ref()
                != Some(&(expected.record.admission.clone(), expected.exposure.clone()))
            {
                return Err(NoticeRejection::StaleVisibility);
            }
            if self.entries[index].content.dismissal == NoticeDismissal::Persistent {
                return Err(NoticeRejection::Persistent);
            }
            self.entries.remove(index);
            self.diagnostics.dismissed = self.diagnostics.dismissed.saturating_add(1);
            self.refresh_visible();
            Ok(())
        })();
        self.count_rejection(result)
    }

    pub fn remove(&mut self, expected: &NoticeRecordToken) -> Result<(), NoticeRejection> {
        let result = self.find_exact(expected).map(|index| {
            self.entries.remove(index);
            self.diagnostics.removed = self.diagnostics.removed.saturating_add(1);
            self.refresh_visible();
        });
        self.count_rejection(result)
    }

    pub fn dispose(&mut self) {
        self.entries = Vec::new();
        self.visible = None;
        self.diagnostics.disposed = true;
    }

    pub fn contains(&self, expected: &NoticeRecordToken) -> bool {
        self.find_exact(expected).is_ok()
    }

    pub fn diagnostics(&self) -> NoticeDiagnostics {
        NoticeDiagnostics {
            retained_records: self.entries.len(),
            pending_records: self
                .entries
                .len()
                .saturating_sub(usize::from(self.visible.is_some())),
            retained_text_bytes: self
                .entries
                .iter()
                .map(|entry| entry.content.retained_text_bytes())
                .sum(),
            ..self.diagnostics
        }
    }

    fn validate_record(&self, record: &NoticeRecord) -> Result<(), NoticeRejection> {
        if self.diagnostics.disposed {
            return Err(NoticeRejection::Disposed);
        }
        if record.window_id != self.window_id {
            return Err(NoticeRejection::WrongWindow);
        }
        if matches!(
            record.kind,
            NoticeKind::HomeFailure | NoticeKind::RuntimeUnavailable
        ) && record.content.dismissal != NoticeDismissal::Persistent
        {
            return Err(NoticeRejection::Persistent);
        }
        Ok(())
    }

    fn update_entry(
        &mut self,
        index: usize,
        record: NoticeRecord,
    ) -> Result<NoticeRecordToken, NoticeRejection> {
        self.validate_record(&record)?;
        let entry = &mut self.entries[index];
        if entry.kind != record.kind {
            return Err(NoticeRejection::ConditionKindMismatch);
        }
        if record.revision <= entry.token.revision {
            return Err(NoticeRejection::StaleRevision);
        }
        entry.token.revision = record.revision;
        entry.content = record.content;
        entry.report_count = entry.report_count.saturating_add(1);
        self.diagnostics.updated = self.diagnostics.updated.saturating_add(1);
        Ok(entry.token.clone())
    }

    fn insert(&mut self, record: NoticeRecord) -> NoticeRecordToken {
        let token = NoticeRecordToken {
            owner: self.owner.clone(),
            admission: Identity::new(),
            window_id: self.window_id,
            condition: record.condition,
            revision: record.revision,
        };
        self.entries.push(Entry {
            token: token.clone(),
            kind: record.kind,
            content: record.content,
            report_count: 1,
        });
        self.diagnostics.admitted = self.diagnostics.admitted.saturating_add(1);
        token
    }

    fn find_exact(&self, expected: &NoticeRecordToken) -> Result<usize, NoticeRejection> {
        if self.diagnostics.disposed {
            return Err(NoticeRejection::Disposed);
        }
        if expected.window_id != self.window_id {
            return Err(NoticeRejection::WrongWindow);
        }
        if expected.owner != self.owner {
            return Err(NoticeRejection::StaleRecord);
        }
        self.entries
            .iter()
            .position(|entry| entry.token == *expected)
            .ok_or(NoticeRejection::StaleRecord)
    }

    fn is_active(&self, entry: &Entry) -> bool {
        self.visible
            .as_ref()
            .is_some_and(|(admission, _)| *admission == entry.token.admission)
    }

    fn refresh_visible(&mut self) {
        // Entries retain admission order through preemption and content updates.
        let next = self
            .entries
            .iter()
            .enumerate()
            .min_by_key(|(index, entry)| (entry.kind, *index))
            .map(|(_, entry)| entry.token.admission.clone());
        if self.visible.as_ref().map(|(admission, _)| admission) != next.as_ref() {
            self.visible = next.map(|admission| (admission, Identity::new()));
        }
    }

    fn reject_admission(&mut self, reason: NoticeRejection) -> NoticeAdmission {
        self.diagnostics.rejected = self.diagnostics.rejected.saturating_add(1);
        NoticeAdmission::Rejected(reason)
    }

    fn count_rejection<T>(
        &mut self,
        result: Result<T, NoticeRejection>,
    ) -> Result<T, NoticeRejection> {
        if result.is_err() {
            self.diagnostics.rejected = self.diagnostics.rejected.saturating_add(1);
        }
        result
    }
}
