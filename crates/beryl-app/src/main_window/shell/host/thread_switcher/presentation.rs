use super::*;

pub(super) fn activity(time: Option<beryl_state::UnixMillis>) -> String {
    let Some(time) = time else {
        return "No activity".into();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| {
            u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
        });
    let minutes = now.saturating_sub(time.get()) / 60_000;
    match minutes {
        0 => "Just now".into(),
        1..=59 => format!("{minutes} min ago"),
        60..=1439 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1440),
    }
}
fn unavailable(availability: beryl_model::Availability) -> Option<String> {
    match availability {
        beryl_model::Availability::Unavailable(reason) => {
            Some(format!("This runtime or root is unavailable: {reason:?}."))
        }
        _ => None,
    }
}
impl MainWindowShellRoot {
    pub(super) fn switcher_thread_row(
        &mut self,
        row: &beryl_state::CatalogQueryRow,
        cx: &Context<Self>,
    ) -> PickerRow {
        let catalog = row.catalog();
        let facts = catalog.facts();
        let execution = facts.execution();
        let current = self.coherent_viewed_thread(cx) == Some(catalog.thread_id());
        let invoking = self.controller().map(|controller| controller.window_id());
        let elsewhere = facts
            .claim()
            .window_id()
            .is_some_and(|id| Some(id) != invoking);
        let reason = if current {
            None
        } else if elsewhere {
            Some(
                "This thread is open in another window. One thread cannot be open in two windows."
                    .into(),
            )
        } else {
            unavailable(execution.availability().runtime())
                .or_else(|| unavailable(execution.availability().root()))
        };
        let state = if current {
            "current".into()
        } else if elsewhere {
            "open elsewhere".into()
        } else {
            activity(Some(facts.last_activity_at()))
        };
        let secondary = if matches!(self.thread_switcher.mode, SwitcherMode::Root { .. }) {
            state
        } else {
            let executable = if row.runtime_environment_count() > 1 {
                format!(" - {}", execution.configured_executable_path().as_str())
            } else {
                String::new()
            };
            format!(
                "{}{executable} - {} - {state}",
                execution.environment_label(),
                execution.full_root_path().as_str()
            )
        };
        let primary = catalog
            .title()
            .text()
            .unwrap_or("Untitled thread")
            .to_owned();
        let key = PickerRowKey(format!("thread-{:?}", catalog.thread_id()));
        self.thread_switcher
            .threads
            .retain(|(resident, _, _)| resident != &key);
        self.thread_switcher
            .threads
            .push_back((key.clone(), catalog.thread_id(), reason.clone()));
        while self.thread_switcher.threads.len() > ROW_LIMIT {
            self.thread_switcher.threads.pop_front();
        }
        PickerRow {
            key,
            primary,
            tooltip: Some(secondary.clone()),
            secondary,
            status: if reason.is_some() {
                "UNAVAILABLE".into()
            } else if current {
                "OPEN".into()
            } else {
                String::new()
            },
            unavailable_reason: reason,
            current,
            activation_pending: false,
        }
    }
    pub(super) fn switcher_root_row(&mut self, row: &beryl_state::CatalogRootRow) -> PickerRow {
        let key = PickerRowKey(format!(
            "root-{:?}-{:?}",
            row.runtime().runtime_id(),
            row.root().root_id()
        ));
        self.thread_switcher
            .roots
            .retain(|(resident, _)| resident != &key);
        self.thread_switcher
            .roots
            .push_back((key.clone(), row.clone()));
        while self.thread_switcher.roots.len() > ROW_LIMIT {
            self.thread_switcher.roots.pop_front();
        }
        let path = row.root().display_path().as_str().to_owned();
        PickerRow {
            key,
            primary: path.clone(),
            secondary: format!(
                "{} threads - {}",
                row.thread_count(),
                activity(row.root().last_activity_at())
            ),
            status: "Choose".into(),
            tooltip: Some(path),
            unavailable_reason: None,
            current: false,
            activation_pending: false,
        }
    }
    pub(super) fn switcher_runtime_row(
        &mut self,
        row: &beryl_state::CatalogRuntimeRow,
    ) -> PickerRuntimeRow {
        let runtime = row.runtime();
        let key = PickerRowKey(format!("runtime:{}", runtime.runtime_id()));
        let active = matches!(&self.thread_switcher.mode, SwitcherMode::Roots(scoped) if scoped.runtime_id() == runtime.runtime_id());
        self.thread_switcher
            .runtimes
            .retain(|(resident, _)| resident != &key);
        self.thread_switcher
            .runtimes
            .push_back((key.clone(), row.clone()));
        while self.thread_switcher.runtimes.len() > ROW_LIMIT {
            self.thread_switcher.runtimes.pop_front();
        }
        let readiness = match runtime.availability().availability() {
            beryl_model::Availability::Available => "Ready",
            beryl_model::Availability::Unknown => "Not checked",
            _ => "Unavailable",
        };
        let path = runtime.canonical_executable().as_str();
        PickerRuntimeRow {
            row: PickerRow {
                key,
                primary: runtime.environment_label().into(),
                secondary: format!("{path} - {} roots - {readiness}", row.root_count()),
                tooltip: Some(path.into()),
                status: String::new(),
                unavailable_reason: None,
                current: false,
                activation_pending: false,
            },
            browse_roots: if active {
                PickerCommandState::unavailable("Roots shown", "These roots are already shown.")
            } else {
                PickerCommandState::enabled("Browse roots")
            },
            add_root: PickerCommandState::enabled("Add root"),
            active_scope: active,
        }
    }
}
