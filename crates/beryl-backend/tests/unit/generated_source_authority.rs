use super::*;
use crate::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError,
};

struct ObserveClose {
    authority: Arc<LoadedSourceAuthority>,
    expected_live: bool,
    refuse: bool,
}
impl OrderedTurnStreamSink for ObserveClose {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        assert_eq!(
            self.authority.live.load(Ordering::Acquire),
            self.expected_live
        );
        if self.refuse {
            Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Unavailable,
            ))
        } else {
            Ok(OrderedTurnStreamCompletion::Applied)
        }
    }
}
fn authority(thread: &str) -> Arc<LoadedSourceAuthority> {
    Arc::new(LoadedSourceAuthority {
        thread: CasThreadId::new(thread).unwrap(),
        incarnation: GeneratedSourceIncarnation::from_bytes([7; 32]),
        live: AtomicBool::new(true),
    })
}

#[test]
fn exact_ordered_thread_close_revokes_source_before_forwarding_even_on_rejection() {
    let authority = authority("loaded");
    let mut inner = ObserveClose {
        authority: authority.clone(),
        expected_live: true,
        refuse: false,
    };
    LoadedSourceSink {
        sink: &mut inner,
        authority: Some(authority.clone()),
    }
    .submit(OrderedTurnStreamOperation::ThreadClosed(
        crate::ThreadClosed::decoded(CasThreadId::new("other").unwrap()),
    ))
    .unwrap();
    assert!(authority.live.load(Ordering::Acquire));
    inner.expected_live = false;
    inner.refuse = true;
    assert!(
        LoadedSourceSink {
            sink: &mut inner,
            authority: Some(authority.clone())
        }
        .submit(OrderedTurnStreamOperation::ThreadClosed(
            crate::ThreadClosed::decoded(CasThreadId::new("loaded").unwrap())
        ))
        .is_err()
    );
    assert!(!authority.live.load(Ordering::Acquire));
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn replacing_a_loaded_projection_irrevocably_retires_all_prior_proof_clones() {
    let mut session =
        ManagedBackendSession::unsupported_streamed_input_gate_for_lifecycle_test().unwrap();
    let mut loaded = crate::thread_lineage::fresh_idle_thread_for_lifecycle_test(
        CasThreadId::new("loaded").unwrap(),
    )
    .into_loaded();
    let first = authority("loaded");
    loaded.generated_source_authority = Some(first.clone());
    session.generated_loaded_source = Some(first.clone());
    let old = loaded.clone();
    assert_eq!(old.generated_source_incarnation(), Some(first.incarnation));
    session.register_generated_loaded_source(&mut loaded);
    assert!(!first.live.load(Ordering::Acquire));
    assert_eq!(loaded.generated_source_incarnation(), None);
    assert_eq!(old.generated_source_incarnation(), None);
    let current = authority("loaded");
    loaded.generated_source_authority = Some(current.clone());
    session.generated_loaded_source = Some(current.clone());
    assert!(current.live.load(Ordering::Acquire));
    session.retire_generated_thread(&CasThreadId::new("other").unwrap());
    assert!(current.live.load(Ordering::Acquire));
    session.retire_generated_thread(loaded.thread_id());
    assert!(!current.live.load(Ordering::Acquire));
    assert!(
        !old.generated_source_authority
            .as_ref()
            .unwrap()
            .live
            .load(Ordering::Acquire)
    );
}

#[test]
fn entropy_failure_never_publishes_partially_filled_incarnation() {
    let thread = CasThreadId::new("loaded").unwrap();
    assert!(mint_loaded_source(thread.clone(), |bytes| {
        bytes.fill(9);
        Err(())
    }).is_none());
    let first = mint_loaded_source(thread.clone(), getrandom::fill).unwrap();
    let second = mint_loaded_source(thread, getrandom::fill).unwrap();
    assert_ne!(first.incarnation, second.incarnation);
}

#[cfg(feature = "lifecycle-test-support")]
#[test]
fn lifecycle_loaded_session_never_mints_generated_source_authority() {
    let mut session = ManagedBackendSession::unsupported_streamed_input_gate_for_lifecycle_test().unwrap();
    let mut loaded = crate::thread_lineage::fresh_idle_thread_for_lifecycle_test(
        CasThreadId::new("loaded").unwrap(),
    ).into_loaded();
    assert_eq!(loaded.generated_source_incarnation(), None);
    session.register_generated_loaded_source(&mut loaded);
    assert_eq!(loaded.generated_source_incarnation(), None);
    assert!(session.generated_loaded_source.is_none());
}

#[test]
fn process_retirement_closes_its_actual_open_file_and_is_idempotent() {
    let process = ProcessSourceFiles::default();
    let file = Arc::new(HostFile {
        file: Mutex::new(Some(tempfile::tempfile().unwrap())),
    });
    lock(&process.files).push(Arc::downgrade(&file));
    assert!(lock(&file.file).is_some());
    process.retire();
    assert!(process.retired.load(Ordering::Acquire));
    assert!(lock(&file.file).is_none());
    process.retire();
}

#[test]
fn bounded_source_failures_survive_the_read_error_adapter_without_path_details() {
    for failure in [
        GeneratedOutputError::Cancelled,
        GeneratedOutputError::LostSourceAuthority,
        GeneratedOutputError::Changed,
        GeneratedOutputError::CleanupPending,
    ] {
        let error = failure.into_io_error();
        assert_eq!(
            error
                .get_ref()
                .unwrap()
                .downcast_ref::<GeneratedOutputError>(),
            Some(&failure)
        );
        assert!(error.to_string().len() < 100);
    }
    assert_eq!(
        GeneratedReaderAllowance::new(0),
        Err(GeneratedOutputError::Unsupported)
    );
    assert_eq!(GeneratedReaderAllowance::new(2).unwrap().0, 2);
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        GeneratedReaderAllowance::new(u32::MAX as usize + 1),
        Err(GeneratedOutputError::Unsupported)
    );
}

#[test]
fn host_file_admission_uses_current_host_absolute_policy_and_preserves_unc() {
    use beryl_model::{PathFlavor, RuntimeMode};
    let windows = RuntimeNativePath::from_admitted(
        RuntimeMode::Host,
        PathFlavor::Windows,
        r"C:\output\original.png",
    )
    .unwrap();
    let unc = RuntimeNativePath::from_admitted(
        RuntimeMode::Host,
        PathFlavor::Windows,
        r"\\server\share\original.png",
    )
    .unwrap();
    let posix = RuntimeNativePath::from_admitted(
        RuntimeMode::Host,
        PathFlavor::Posix,
        "/output/original.png",
    )
    .unwrap();
    assert_eq!(
        crate::generated_output::source::host_path_is_native(&windows),
        cfg!(target_os = "windows")
    );
    assert_eq!(
        crate::generated_output::source::host_path_is_native(&unc),
        cfg!(target_os = "windows")
    );
    assert_eq!(
        crate::generated_output::source::host_path_is_native(&posix),
        !cfg!(target_os = "windows")
    );
}
