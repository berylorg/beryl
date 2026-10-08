use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn primary_thread_reason(
        &self,
        cx: &App,
    ) -> Option<String> {
        if self
            .controller()
            .is_some_and(|controller| controller.is_threadless())
        {
            Some("Add a runtime with the … button before creating a thread.".into())
        } else if let Some(error) = &self.runtime_setup.unavailable {
            Some(error.clone())
        } else if self.runtime_setup.pending() || self.running_threads.has_activation_custody() {
            Some("New Thread is waiting for this window's original request.".into())
        } else if !self.setup_enabled() {
            Some("New Thread is waiting for this window's current work.".into())
        } else if !self
            .runtime_setup
            .services
            .as_ref()
            .is_some_and(|services| services.current())
        {
            Some("New Thread services are unavailable.".into())
        } else if self.cached_running_selection(cx).is_none() {
            Some("The selected conversation is unavailable.".into())
        } else {
            None
        }
    }

    pub(in crate::main_window::shell::host) fn begin_primary_thread(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.primary_thread_reason(cx).is_some() {
            return;
        }
        let selected = self
            .cached_running_selection(cx)
            .expect("qualified selection")
            .0;
        #[cfg(all(test, feature = "test-faults"))]
        let fixture = self.runtime_setup.fixture_transcript_reader.clone();
        #[cfg(target_os = "windows")]
        let published = || {
            crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                .and_then(|owner| owner.upgrade())
                .and_then(|owner| owner.borrow().thread_creation_reader())
        };
        #[cfg(all(test, feature = "test-faults", target_os = "windows"))]
        let reader = fixture.or_else(published);
        #[cfg(all(not(all(test, feature = "test-faults")), target_os = "windows"))]
        let reader = published();
        #[cfg(all(test, feature = "test-faults", not(target_os = "windows")))]
        let reader = fixture;
        #[cfg(all(not(all(test, feature = "test-faults")), not(target_os = "windows")))]
        let reader: Option<crate::app_services::PublishedRunningThreadsReader> = None;
        let Some(reader) = reader else {
            self.setup_failure("Thread creation source is unavailable.", false);
            cx.notify();
            return;
        };
        if self.runtime_setup.picker.as_ref().is_some_and(|picker| {
            !picker.update(cx, |picker, cx| {
                picker.set_external_command_reason(
                    Some("New Thread is waiting for this window's original request.".into()),
                    cx,
                )
            })
        }) {
            return;
        }
        self.runtime_setup.primary_command = true;
        let output_release = self.runtime_setup.workers.track(|| ());
        let work = self.runtime_setup.workers.track(move || {
            let result = (|| {
                let observation = reader.observe()?;
                let (home, _, storage) = reader
                    .activation_sources()
                    .ok_or("Thread creation source is unavailable.")?;
                if home.home_id() != selected.binding().home_id()
                    || home.health().generation() != Some(selected.binding().home_generation())
                {
                    return Err("The selected conversation belongs to retired services.".into());
                }
                let execution = storage
                    .thread_execution(
                        &home,
                        selected.claim().thread_id(),
                        syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or("The selected thread's execution binding is unavailable.")?;
                let execution = reader.elect(&observation, || execution.execution().clone())?;
                Self::thread_creation_request(selected, execution)
            })();
            (reader, result, output_release)
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.runtime_setup.primary_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (reader, result, _release) = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.runtime_setup.primary_task = None;
                if !root.runtime_setup.primary_command {
                    return;
                }
                let result = result.and_then(|request| {
                    if !root.setup_enabled()
                        || !reader.current()
                        || root.cached_running_selection(cx).map(|current| current.0)
                            != Some(selected)
                    {
                        return Err(
                            "The selected conversation changed before New Thread admission.".into(),
                        );
                    }
                    Ok(request)
                });
                root.accept_thread_creation_request(result, window, cx);
            });
        }));
        cx.notify();
    }
}
