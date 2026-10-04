use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};
use crate::{
    app_services::PublishedRunningThreadsReader,
    lifecycle_attention::{
        LifecycleAttentionKind, LifecycleAttentionRecord, LifecycleAttentionToken,
    },
    notice_limits::NOTICE_RECORD_CAPACITY,
};

#[derive(Default)]
pub(super) struct LifecycleAttentionNoticeContribution {
    entries: Vec<AttentionNotice>,
}

struct AttentionNotice {
    attention: LifecycleAttentionRecord,
    reader: AttentionSource,
    condition: NoticeConditionId,
    notice: Option<NoticeRecordToken>,
    revision: u64,
    reports: u64,
}

#[derive(Clone)]
enum AttentionSource {
    Published(PublishedRunningThreadsReader),
    #[cfg(feature = "test-faults")]
    Test(std::sync::Weak<crate::lifecycle_attention::ProcessLifecycleAttentionPool>),
}

impl AttentionSource {
    fn current(&self) -> bool {
        match self {
            Self::Published(reader) => reader.current(),
            #[cfg(feature = "test-faults")]
            Self::Test(pool) => pool
                .upgrade()
                .is_some_and(|pool| pool.try_snapshot().is_some()),
        }
    }
    fn same_publication(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Published(left), Self::Published(right)) => left.same_publication(right),
            #[cfg(feature = "test-faults")]
            (Self::Test(left), Self::Test(right)) => left.ptr_eq(right),
            #[cfg(feature = "test-faults")]
            _ => false,
        }
    }
    fn acknowledge(&self, token: &LifecycleAttentionToken) -> bool {
        match self {
            Self::Published(reader) => reader.acknowledge(token),
            #[cfg(feature = "test-faults")]
            Self::Test(pool) => pool.upgrade().is_some_and(|pool| pool.acknowledge(token)),
        }
    }
}

impl MainWindowNoticeIngress {
    pub(crate) fn offer_lifecycle_attention(
        &self,
        record: &LifecycleAttentionRecord,
        reader: &PublishedRunningThreadsReader,
        app: &mut App,
    ) -> bool {
        self.offer_lifecycle_attention_source(
            record,
            &AttentionSource::Published(reader.clone()),
            app,
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_offer_lifecycle_attention(
        &self,
        record: &LifecycleAttentionRecord,
        pool: &std::sync::Arc<crate::lifecycle_attention::ProcessLifecycleAttentionPool>,
        app: &mut App,
    ) -> bool {
        self.offer_lifecycle_attention_source(
            record,
            &AttentionSource::Test(std::sync::Arc::downgrade(pool)),
            app,
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_remove_lifecycle_attention(&self, token: &LifecycleAttentionToken, app: &mut App) {
        self.remove_lifecycle_attention(token, app);
    }

    fn offer_lifecycle_attention_source(
        &self,
        record: &LifecycleAttentionRecord,
        reader: &AttentionSource,
        app: &mut App,
    ) -> bool {
        if !reader.current() {
            return false;
        }
        self.window
            .update(app, |root, window, cx| {
                if self.validate(root).is_err() || !reader.current() {
                    return false;
                }
                let contribution = &mut root.notices.lifecycle_attention;
                let index = if let Some(index) = contribution
                    .entries
                    .iter()
                    .position(|entry| entry.attention.token() == record.token())
                {
                    index
                } else {
                    if contribution.entries.len() == NOTICE_RECORD_CAPACITY {
                        return false;
                    }
                    contribution.entries.push(AttentionNotice {
                        attention: record.clone(),
                        reader: reader.clone(),
                        condition: NoticeConditionId::new(),
                        notice: None,
                        revision: 0,
                        reports: 0,
                    });
                    contribution.entries.len() - 1
                };
                let entry = &mut contribution.entries[index];
                let arbiter = &mut root.notices.arbiter;
                if entry.reports == record.report_count() && entry.reader.same_publication(reader) {
                    return entry
                        .notice
                        .as_ref()
                        .is_some_and(|notice| arbiter.contains(notice));
                }
                let Some(revision) = entry.revision.checked_add(1) else {
                    return false;
                };
                let content = attention_content(record);
                let notice = if let Some(notice) = entry
                    .notice
                    .as_ref()
                    .filter(|notice| arbiter.contains(notice))
                {
                    arbiter.update(notice, revision, content).ok()
                } else {
                    match arbiter.admit(NoticeRecord {
                        window_id: self.window_id,
                        condition: entry.condition.clone(),
                        revision,
                        kind: NoticeKind::Lifecycle,
                        content,
                    }) {
                        NoticeAdmission::Admitted(notice) | NoticeAdmission::Updated(notice) => {
                            Some(notice)
                        }
                        _ => None,
                    }
                };
                if let Some(notice) = notice {
                    entry.notice = Some(notice);
                    entry.revision = revision;
                }
                entry.reports = record.report_count();
                entry.attention = record.clone();
                entry.reader = reader.clone();
                root.sync_notices(window, cx);
                root.notices
                    .lifecycle_attention
                    .entries
                    .get(index)
                    .and_then(|entry| entry.notice.as_ref())
                    .is_some_and(|notice| root.notices.arbiter.contains(notice))
            })
            .unwrap_or(false)
    }

    pub(crate) fn remove_lifecycle_attention(
        &self,
        token: &LifecycleAttentionToken,
        app: &mut App,
    ) {
        let _ = self.window.update(app, |root, window, cx| {
            if self.validate(root).is_err() {
                return;
            }
            let Some(index) = root
                .notices
                .lifecycle_attention
                .entries
                .iter()
                .position(|entry| entry.attention.token() == token)
            else {
                return;
            };
            let entry = root.notices.lifecycle_attention.entries.remove(index);
            if let Some(notice) = entry.notice
                && root.notices.arbiter.contains(&notice)
            {
                let _ = root.notices.arbiter.remove(&notice);
            }
            root.sync_notices(window, cx);
        });
    }
}

impl MainWindowShellRoot {
    pub(super) fn acknowledge_lifecycle_attention_notice(&mut self, token: &NoticeVisibleToken) {
        let Some(index) = self
            .notices
            .lifecycle_attention
            .entries
            .iter()
            .position(|entry| entry.notice.as_ref() == Some(token.record()))
        else {
            return;
        };
        let entry = self.notices.lifecycle_attention.entries.remove(index);
        entry.reader.acknowledge(entry.attention.token());
    }
}

fn attention_content(record: &LifecycleAttentionRecord) -> NoticeContent {
    let (title, detail, variant) = match record.kind() {
        LifecycleAttentionKind::ReviewReady => (
            "Review ready",
            "The thread is ready for review.",
            NoticeVariant::Info,
        ),
        LifecycleAttentionKind::OperatorAttention => (
            "Operator attention needed",
            "The thread is waiting for operator attention.",
            NoticeVariant::Warning,
        ),
        LifecycleAttentionKind::PlanComplete => (
            "Plan complete",
            "The thread reported plan completion.",
            NoticeVariant::Info,
        ),
        LifecycleAttentionKind::ContinuationFailed => (
            "Continuation failed",
            "The thread could not continue automatically.",
            NoticeVariant::Error,
        ),
    };
    NoticeContent::new(variant, NoticeDismissal::Dismissible, title, detail)
}
