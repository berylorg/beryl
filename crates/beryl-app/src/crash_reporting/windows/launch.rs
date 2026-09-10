use std::{
    io,
    mem::size_of,
    os::windows::{ffi::OsStrExt, io::OwnedHandle},
    path::Path,
};

use windows::{
    Win32::{
        Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
            STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute, WaitForMultipleObjects,
            WaitForSingleObject,
        },
    },
    core::{PCWSTR, PWSTR},
};

use super::{invalid, owned, raw};

pub(super) fn start_ready(executable: &Path, handles: [HANDLE; 3]) -> io::Result<()> {
    let executable = executable.as_os_str().encode_wide().collect::<Vec<_>>();
    if executable.is_empty() || executable.contains(&0) || executable.contains(&u16::from(b'"')) {
        return Err(invalid("invalid crash reporter executable path"));
    }
    let mut command = vec![u16::from(b'"')];
    command.extend_from_slice(&executable);
    command.push(u16::from(b'"'));
    command.extend(
        format!(
            " {} {:x} {:x} {:x}",
            crate::crash_reporting::REPORTER_ARGUMENT,
            handles[0].0 as usize,
            handles[1].0 as usize,
            handles[2].0 as usize,
        )
        .encode_utf16(),
    );
    command.push(0);
    let mut executable = executable;
    executable.push(0);
    let mut size = 0;
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
    if size == 0 || size > 65_536 {
        return Err(invalid("invalid process attribute-list size"));
    }
    let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut size)? };
    let attributes = Attributes(list);
    unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(handles.as_ptr().cast()),
            size_of_val(&handles),
            None,
            None,
        )?;
    }
    let startup = STARTUPINFOEXW {
        StartupInfo: windows::Win32::System::Threading::STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            ..Default::default()
        },
        lpAttributeList: list,
    };
    let mut information = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(executable.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            true,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_BREAKAWAY_FROM_JOB | CREATE_NO_WINDOW,
            None,
            None,
            &startup.StartupInfo,
            &mut information,
        )?;
    }
    let mut child = ProvisionalChild {
        process: owned(information.hProcess),
        stop_on_drop: true,
    };
    drop(owned(information.hThread));
    drop(attributes);
    drop(storage);
    let wait = unsafe { WaitForMultipleObjects(&[handles[2], raw(&child.process)], false, 5_000) };
    if wait != WAIT_OBJECT_0 {
        return Err(io::Error::new(
            if wait == WAIT_TIMEOUT {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::BrokenPipe
            },
            "crash reporter did not become ready",
        ));
    }
    if unsafe { WaitForSingleObject(raw(&child.process), 0) } == WAIT_OBJECT_0 {
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "crash reporter exited during startup",
        ));
    }
    child.stop_on_drop = false;
    Ok(())
}

struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.0) };
    }
}

struct ProvisionalChild {
    process: OwnedHandle,
    stop_on_drop: bool,
}

impl Drop for ProvisionalChild {
    fn drop(&mut self) {
        if self.stop_on_drop {
            let _ = unsafe { TerminateProcess(raw(&self.process), 1) };
            let _ = unsafe { WaitForSingleObject(raw(&self.process), INFINITE) };
        }
    }
}
