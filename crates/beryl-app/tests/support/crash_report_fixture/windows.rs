use std::{
    env, fs,
    io::{self, Read},
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use beryl_app::crash_reporting::{REPORTER_ARGUMENT, install, run_reporter};
use windows::Win32::{
    Foundation::{FILETIME, GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, WAIT_TIMEOUT},
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        },
        Threading::{
            CreateEventW, GetCurrentProcess, GetProcessTimes, SetEvent, WaitForSingleObject,
        },
    },
};

pub fn run() {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    let directory = PathBuf::from(env::var_os("BERYL_CRASH_REPORT_TEST_DIR").unwrap());
    if arguments
        .first()
        .is_some_and(|value| value == REPORTER_ARGUMENT)
    {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        }
        .unwrap();
        let created = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        fs::write(
            directory.join("reporter-pid"),
            format!("{} {created}", std::process::id()),
        )
        .unwrap();
        let mut in_job = false.into();
        unsafe { IsProcessInJob(GetCurrentProcess(), None, &mut in_job) }.unwrap();
        fs::write(
            directory.join("receiver-state"),
            format!("in_job={}", in_job.as_bool()),
        )
        .unwrap();
        if let Some(value) = env::var_os("BERYL_CRASH_REPORT_TEST_HANDLE") {
            let value = usize::from_str_radix(value.to_str().unwrap(), 16).unwrap();
            let handle = HANDLE(value as *mut _);
            let mut flags = 0;
            if unsafe { GetHandleInformation(handle, &mut flags) }.is_ok()
                && flags & HANDLE_FLAG_INHERIT.0 != 0
            {
                let _ = unsafe { SetEvent(handle) };
            }
        }
        match env::var("BERYL_CRASH_REPORT_TEST_STARTUP").as_deref() {
            Ok("exit") => std::process::exit(7),
            Ok("timeout") => thread::sleep(Duration::from_secs(30)),
            _ => {}
        }
        run_reporter(&arguments[1..], |report| {
            fs::write(directory.join("report"), &report).unwrap();
            if env::var_os("BERYL_CRASH_REPORT_TEST_GUI").is_some() {
                beryl_app::crash_reporting::present(report);
                fs::write(directory.join("gui-closed"), "closed").unwrap();
            }
        });
    }

    let mode = arguments.first().unwrap().to_str().unwrap();
    let _job = (mode == "job-refusal").then(block_breakaway);
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    let sentinel = owned(unsafe { CreateEventW(Some(&security), true, false, None) }.unwrap());
    // The fixture is Windows-only; environment mutation is supported here before child startup.
    unsafe {
        env::set_var(
            "BERYL_CRASH_REPORT_TEST_HANDLE",
            format!("{:x}", sentinel.as_raw_handle() as usize),
        );
    }
    let installation = install(&env::current_exe().unwrap());
    let ready = installation.is_ok();
    let clean = unsafe { WaitForSingleObject(HANDLE(sentinel.as_raw_handle()), 0) } == WAIT_TIMEOUT;
    fs::write(
        directory.join("installation"),
        format!("ready={ready} clean={clean} error={installation:?}"),
    )
    .unwrap();
    let mut trigger = [0];
    io::stdin().read_exact(&mut trigger).unwrap();
    match mode {
        "normal" => {}
        "large" => panic!("{}", "🙂".repeat(10_000)),
        "non-string" => std::panic::panic_any(42usize),
        "caught" => {
            let _ = std::panic::catch_unwind(|| panic!("caught panic must be fatal"));
            fs::write(directory.join("continued"), "invalid").unwrap();
        }
        "concurrent" => {
            let barrier = Arc::new(Barrier::new(3));
            for _ in 0..2 {
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    panic!("concurrent fatal panic");
                });
            }
            barrier.wait();
            thread::sleep(Duration::from_secs(10));
            panic!("panic threads did not terminate the process");
        }
        "install-twice" => {
            assert_eq!(
                install(&env::current_exe().unwrap()).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            panic!("original reporter remains installed");
        }
        _ => panic!("storage writer invariant failed"),
    }
}

fn block_breakaway() -> OwnedHandle {
    let job = owned(unsafe { CreateJobObjectW(None, None) }.unwrap());
    let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            HANDLE(job.as_raw_handle()),
            JobObjectExtendedLimitInformation,
            (&information as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .unwrap();
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), GetCurrentProcess()).unwrap();
    }
    job
}

fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}
