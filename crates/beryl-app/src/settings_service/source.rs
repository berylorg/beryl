use std::sync::{
    Arc, Mutex, RwLock, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use beryl_home_store::{HomeCandidateRecoveryAccess, HomeServiceReference, HomeStore};
use beryl_state::{InstalledThemeId, SettingsState};

use super::{draft::validate_value, model::*, operation::*};

pub(super) struct Resources {
    pub home: HomeServiceReference,
    pub settings: SettingsState,
    pub theme: Option<crate::theme_runtime::ThemeSettingsSource>,
}
pub(super) struct ServiceInner {
    pub resources: Mutex<Option<Resources>>,
    pub baseline: RwLock<Arc<SettingsBaseline>>,
    pub owner: SettingsProcessOwner,
    pub retired: AtomicBool,
}

#[derive(Clone)]
pub struct SettingsService {
    pub(super) inner: Arc<ServiceInner>,
}
#[derive(Clone)]
pub struct PublishedSettingsReader {
    inner: Weak<ServiceInner>,
}
#[derive(Clone)]
pub struct AppliedSettingsReader {
    inner: Weak<ServiceInner>,
}
impl std::fmt::Debug for AppliedSettingsReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AppliedSettingsReader")
    }
}

pub struct PreparedSettingsService {
    baseline: Arc<SettingsBaseline>,
    settings: SettingsState,
    inner: Arc<ServiceInner>,
}

impl PreparedSettingsService {
    pub fn prepare_candidate(
        access: &HomeCandidateRecoveryAccess<'_>,
        settings: &SettingsState,
        owner: SettingsProcessOwner,
        service_generation: u64,
    ) -> Result<Self, SettingsServiceError> {
        owner.check_home(access.home_id())?;
        owner.reconcile_candidate_original(access, settings)?;
        let (revision, records) = read_candidate(access, settings)?;
        let identity = SettingsSourceIdentity {
            home_id: access.home_id(),
            home_generation: access.generation(),
            service_generation,
            settings_revision: revision,
            publication: 1,
        };
        let baseline = Arc::new(build_baseline(identity, records, owner.excluded()?)?);
        let inner = Arc::new(ServiceInner {
            resources: Mutex::new(None),
            baseline: RwLock::new(Arc::clone(&baseline)),
            owner: owner.clone(),
            retired: AtomicBool::new(true),
        });
        Ok(Self {
            baseline,
            settings: settings.clone(),
            inner,
        })
    }
    pub fn baseline(&self) -> &Arc<SettingsBaseline> {
        &self.baseline
    }
    pub fn applied_reader(&self) -> AppliedSettingsReader {
        AppliedSettingsReader {
            inner: Arc::downgrade(&self.inner),
        }
    }
    pub fn release_published(
        self,
        home: HomeServiceReference,
    ) -> Result<SettingsService, SettingsServiceError> {
        if home.home_id() != self.baseline.identity.home_id
            || home.health().generation() != Some(self.baseline.identity.home_generation)
        {
            return Err(SettingsServiceError::Stale);
        }
        let mut baseline = (*self.baseline).clone();
        baseline.applied.home_binding = Some(home.generation_identity()?);
        *self
            .inner
            .baseline
            .write()
            .map_err(|_| SettingsServiceError::Custody)? = Arc::new(baseline);
        *self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)? = Some(Resources {
            home,
            settings: self.settings,
            theme: None,
        });
        self.inner.retired.store(false, Ordering::Release);
        Ok(SettingsService { inner: self.inner })
    }
}

impl SettingsService {
    pub fn prepare(
        home: HomeServiceReference,
        settings: SettingsState,
        owner: SettingsProcessOwner,
        service_generation: u64,
    ) -> Result<Self, SettingsServiceError> {
        owner.check_home(home.home_id())?;
        if owner.pending_operation().is_some() {
            return Err(SettingsServiceError::Busy);
        }
        home.generation_identity()?;
        let generation = home
            .health()
            .generation()
            .ok_or(SettingsServiceError::Retired)?;
        let (revision, records) = read_store(&home, &settings)?;
        let identity = SettingsSourceIdentity {
            home_id: home.home_id(),
            home_generation: generation,
            service_generation,
            settings_revision: revision,
            publication: 1,
        };
        let mut baseline = build_baseline(identity, records, owner.excluded()?)?;
        baseline.applied.home_binding = Some(home.generation_identity()?);
        let baseline = Arc::new(baseline);
        Ok(Self::from_parts(home, settings, owner, baseline))
    }
    fn from_parts(
        home: HomeServiceReference,
        settings: SettingsState,
        owner: SettingsProcessOwner,
        baseline: Arc<SettingsBaseline>,
    ) -> Self {
        Self {
            inner: Arc::new(ServiceInner {
                resources: Mutex::new(Some(Resources {
                    home,
                    settings,
                    theme: None,
                })),
                baseline: RwLock::new(baseline),
                owner,
                retired: AtomicBool::new(false),
            }),
        }
    }
    pub fn reader(&self) -> PublishedSettingsReader {
        PublishedSettingsReader {
            inner: Arc::downgrade(&self.inner),
        }
    }
    pub fn bind_theme_source(
        &self,
        source: crate::theme_runtime::ThemeSettingsSource,
    ) -> Result<(), SettingsServiceError> {
        if self.inner.retired.load(Ordering::Acquire) {
            return Err(SettingsServiceError::Retired);
        }
        let mut resources = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        let resources = resources.as_mut().ok_or(SettingsServiceError::Retired)?;
        if resources.theme.is_some()
            || source.home().home_id() != resources.home.home_id()
            || Some(source.home().home_generation()) != resources.home.health().generation()
        {
            return Err(SettingsServiceError::Theme);
        }
        resources.theme = Some(source);
        Ok(())
    }
    pub fn applied_reader(&self) -> AppliedSettingsReader {
        AppliedSettingsReader {
            inner: Arc::downgrade(&self.inner),
        }
    }
    pub fn snapshot(&self) -> Result<Arc<SettingsBaseline>, SettingsServiceError> {
        checked_snapshot(&self.inner)
    }
    pub fn is_reconciling(&self) -> bool {
        self.inner.owner.pending_operation().is_some()
    }
    pub fn pending_operation(&self) -> Option<u64> {
        self.inner.owner.pending_operation()
    }
    pub fn retire(&self) -> Result<(), SettingsServiceError> {
        self.inner.retired.store(true, Ordering::Release);
        self.inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .take();
        self.inner.owner.retire_preparation()?;
        Ok(())
    }
}

fn checked_snapshot(inner: &ServiceInner) -> Result<Arc<SettingsBaseline>, SettingsServiceError> {
    if inner.retired.load(Ordering::Acquire) {
        return Err(SettingsServiceError::Retired);
    }
    let baseline = inner
        .baseline
        .read()
        .map_err(|_| SettingsServiceError::Custody)?;
    if inner.retired.load(Ordering::Acquire) {
        return Err(SettingsServiceError::Retired);
    }
    Ok(Arc::clone(&baseline))
}

impl PublishedSettingsReader {
    pub fn snapshot(&self) -> Result<Arc<SettingsBaseline>, SettingsServiceError> {
        let inner = self.inner.upgrade().ok_or(SettingsServiceError::Retired)?;
        checked_snapshot(&inner)
    }
    pub fn invalidation_after(
        &self,
        identity: SettingsSourceIdentity,
    ) -> Result<Option<SettingsSourceIdentity>, SettingsServiceError> {
        let current = self.snapshot()?.identity();
        Ok((current != identity).then_some(current))
    }
}
impl AppliedSettingsReader {
    pub fn same_source(&self, other: &Self) -> bool {
        self.inner.ptr_eq(&other.inner)
    }
    pub fn snapshot(&self) -> Result<AppliedSettingsSnapshot, SettingsServiceError> {
        let inner = self.inner.upgrade().ok_or(SettingsServiceError::Retired)?;
        Ok(checked_snapshot(&inner)?.applied.clone())
    }
    pub fn invalidation_after(
        &self,
        identity: SettingsSourceIdentity,
    ) -> Result<Option<SettingsSourceIdentity>, SettingsServiceError> {
        let current = self.snapshot()?.identity;
        Ok((current != identity).then_some(current))
    }
}

pub(super) fn read_store(
    home: &HomeStore,
    settings: &SettingsState,
) -> Result<
    (
        beryl_model::DomainRevision,
        [Option<SettingsRecordFacts>; 5],
    ),
    SettingsServiceError,
> {
    let revision = settings.revision(home)?;
    let mut records = std::array::from_fn(|_| None);
    for key in SETTINGS_KEYS {
        records[index(key)] = settings
            .setting(home, key)?
            .map(SettingsRecordFacts::from_record);
    }
    if settings.revision(home)? != revision {
        return Err(SettingsServiceError::Changed);
    }
    Ok((revision, records))
}
pub(super) fn read_candidate(
    access: &HomeCandidateRecoveryAccess<'_>,
    settings: &SettingsState,
) -> Result<
    (
        beryl_model::DomainRevision,
        [Option<SettingsRecordFacts>; 5],
    ),
    SettingsServiceError,
> {
    let revision = settings.revision_candidate(access)?;
    let mut records = std::array::from_fn(|_| None);
    for key in SETTINGS_KEYS {
        records[index(key)] = settings
            .setting_candidate(access, key)?
            .map(SettingsRecordFacts::from_record);
    }
    if settings.revision_candidate(access)? != revision {
        return Err(SettingsServiceError::Changed);
    }
    Ok((revision, records))
}

pub(super) fn build_baseline(
    identity: SettingsSourceIdentity,
    records: [Option<SettingsRecordFacts>; 5],
    excluded: u8,
) -> Result<SettingsBaseline, SettingsServiceError> {
    let mut rows = SETTINGS_KEYS.map(|key| SettingsRow {
        key,
        value: default_text(key).into(),
        availability: SettingsRowAvailability::Available,
        issue: None,
    });
    let mut origins = SETTINGS_KEYS.map(|key| {
        if records[index(key)].is_none() {
            AppliedSettingOrigin::AbsentDefault
        } else {
            AppliedSettingOrigin::Saved
        }
    });
    for key in SETTINGS_KEYS {
        let row = &mut rows[index(key)];
        if let Some(record) = &records[index(key)] {
            let text = (record.schema == key.schema_version())
                .then(|| presentation(key, &record.value))
                .flatten();
            if let Some(text) = text.filter(|text| validate_value(key, text).is_ok()) {
                row.value = text;
            } else {
                row.availability = SettingsRowAvailability::RejectedSaved;
                row.issue = Some(SettingsIssue::RejectedSaved);
                origins[index(key)] = AppliedSettingOrigin::RejectedSaved;
            }
        }
        if excluded & mask(key) != 0 {
            row.availability = SettingsRowAvailability::Excluded;
            row.issue = Some(SettingsIssue::Excluded);
        }
    }
    let active_theme = if rows[0].value.is_empty() {
        None
    } else {
        InstalledThemeId::new(&rows[0].value).ok()
    };
    let timeout = rows[1].value.parse::<u64>().unwrap_or(180);
    let autosave = rows[2].value.parse::<u64>().unwrap_or(30);
    let sound = validate_value(SettingKey::EndTurnSound, &rows[4].value)
        .ok()
        .and_then(|value| value.as_end_turn_sound().cloned())
        .flatten();
    let applied = AppliedSettingsSnapshot {
        identity,
        excluded,
        active_theme,
        origins,
        home_binding: None,
        timeout: Duration::from_secs(timeout),
        autosave: Duration::from_secs(autosave),
        instructions: Arc::from(rows[3].value.as_str()),
        sound,
    };
    Ok(SettingsBaseline {
        identity,
        records,
        rows,
        applied,
    })
}
fn default_text(key: SettingKey) -> &'static str {
    match key {
        SettingKey::ContextCompactionTimeout => "180",
        SettingKey::DraftAutosaveInterval => "30",
        _ => "",
    }
}
fn presentation(key: SettingKey, value: &beryl_state::SettingValue) -> Option<String> {
    match key {
        SettingKey::ActiveThemeId => value.as_active_theme_id().map(str::to_owned),
        SettingKey::ContextCompactionTimeout => value
            .as_context_compaction_timeout_millis()
            .filter(|ms| ms % 1000 == 0)
            .map(|ms| (ms / 1000).to_string()),
        SettingKey::DraftAutosaveInterval => value
            .as_draft_autosave_interval_seconds()
            .map(|v| v.to_string()),
        SettingKey::DeveloperInstructions => value.as_developer_instructions().map(str::to_owned),
        SettingKey::EndTurnSound => value
            .as_end_turn_sound()
            .map(|path| path.as_ref().map(|p| p.as_str()).unwrap_or("").to_owned()),
    }
}
