use gpui::{App, Global};

use crate::composer_host::ComposerHostAutosaveInterval;
use crate::settings_service::{AppliedSettingsReader, SettingsSourceIdentity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsAutosavePublication {
    source: SettingsSourceIdentity,
    publication: u64,
    interval: Option<ComposerHostAutosaveInterval>,
}

impl SettingsAutosavePublication {
    pub const fn source(self) -> SettingsSourceIdentity {
        self.source
    }

    pub const fn publication(self) -> u64 {
        self.publication
    }

    pub const fn interval(self) -> Option<ComposerHostAutosaveInterval> {
        self.interval
    }
}

pub struct SettingsAutosaveProjection {
    reader: AppliedSettingsReader,
    current: SettingsAutosavePublication,
}

impl Global for SettingsAutosaveProjection {}

pub fn publish_applied(reader: &AppliedSettingsReader, app: &mut App) -> Result<bool, String> {
    let snapshot = reader.snapshot();
    let prior = app.try_global::<SettingsAutosaveProjection>();
    let (source, interval) = match snapshot {
        Ok(snapshot) => {
            let interval = match snapshot.draft_autosave_interval() {
                Ok(duration) => Some(
                    u16::try_from(duration.as_secs())
                        .ok()
                        .filter(|_| duration.subsec_nanos() == 0)
                        .and_then(ComposerHostAutosaveInterval::new)
                        .ok_or_else(|| "applied draft autosave interval is invalid".to_owned())?,
                ),
                Err(_) => None,
            };
            (snapshot.identity, interval)
        }
        Err(error) => match prior {
            Some(prior) if prior.reader.same_source(reader) => (prior.current.source, None),
            _ => return Err(error.to_string()),
        },
    };
    let publication = if let Some(prior) = prior {
        let previous = prior.current.source;
        if source.home_id != previous.home_id {
            return Err("autosave Settings projection belongs to another home".to_owned());
        }
        if prior.reader.same_source(reader) {
            if source.home_generation != previous.home_generation
                || source.service_generation != previous.service_generation
                || source.publication < previous.publication
                || (source.publication == previous.publication && source != previous)
            {
                return Err("autosave Settings publication is stale".to_owned());
            }
            if source == previous && interval == prior.current.interval {
                return Ok(false);
            }
        } else if (source.home_generation, source.service_generation)
            <= (previous.home_generation, previous.service_generation)
        {
            return Err("autosave Settings source replacement is stale".to_owned());
        }
        prior.current.publication.checked_add(1)
    } else {
        Some(1)
    }
    .ok_or_else(|| "autosave Settings publication exhausted".to_owned())?;
    app.set_global(SettingsAutosaveProjection {
        reader: reader.clone(),
        current: SettingsAutosavePublication {
            source,
            publication,
            interval,
        },
    });
    Ok(true)
}

pub fn current(app: &App) -> Option<SettingsAutosavePublication> {
    let projection = app.try_global::<SettingsAutosaveProjection>()?;
    let mut current = projection.current;
    if !projection.reader.snapshot().is_ok_and(|snapshot| {
        snapshot.identity == current.source && snapshot.draft_autosave_interval().is_ok()
    }) {
        current.interval = None;
    }
    Some(current)
}
