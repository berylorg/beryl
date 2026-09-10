use std::{
    ffi::OsString,
    io,
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    panic::PanicHookInfo,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use windows::Win32::{
    Foundation::{
        DUPLICATE_HANDLE_OPTIONS, DuplicateHandle, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS,
        INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0,
    },
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::IsProcessInJob,
        Memory::{
            CreateFileMappingW, FILE_MAP, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS,
            MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
        },
        Threading::{
            CreateEventW, GetCurrentProcess, INFINITE, PROCESS_SYNCHRONIZE, SetEvent,
            WaitForSingleObject,
        },
    },
};

use super::record::ReportRecord;

mod launch;

pub(super) struct Sender {
    view: View,
    claimed: AtomicBool,
}

impl Sender {
    pub(super) fn publish(&self, panic: &PanicHookInfo<'_>) {
        if self.claimed.swap(true, Ordering::Relaxed) {
            return;
        }
        // Only the claim winner writes; the other process reads after this process exits.
        unsafe { &mut *self.view.address.Value.cast::<ReportRecord>() }.capture(panic);
    }
}

struct View {
    address: MEMORY_MAPPED_VIEW_ADDRESS,
}

// The mapping never moves, and Sender's one-shot claim serializes all writes.
unsafe impl Send for View {}
unsafe impl Sync for View {}

impl View {
    fn open(handle: HANDLE, access: FILE_MAP) -> io::Result<Self> {
        let address = unsafe { MapViewOfFile(handle, access, 0, 0, size_of::<ReportRecord>()) };
        if address.Value.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { address })
        }
    }
}

impl Drop for View {
    fn drop(&mut self) {
        let _ = unsafe { UnmapViewOfFile(self.address) };
    }
}

pub(super) fn start(executable: &Path) -> io::Result<Sender> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    let mapping = owned(unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            Some(&security),
            PAGE_READWRITE,
            0,
            size_of::<ReportRecord>() as u32,
            None,
        )
    }?);
    let view = View::open(raw(&mapping), FILE_MAP_WRITE)?;
    unsafe {
        view.address
            .Value
            .cast::<ReportRecord>()
            .write(ReportRecord::empty())
    };
    let ready = owned(unsafe { CreateEventW(Some(&security), true, false, None) }?);
    let mut parent = HANDLE::default();
    unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            GetCurrentProcess(),
            GetCurrentProcess(),
            &mut parent,
            PROCESS_SYNCHRONIZE.0,
            true,
            DUPLICATE_HANDLE_OPTIONS(0),
        )?;
    }
    let parent = owned(parent);
    launch::start_ready(executable, [raw(&mapping), raw(&parent), raw(&ready)])?;
    Ok(Sender {
        view,
        claimed: AtomicBool::new(false),
    })
}

pub(super) fn receive(arguments: &[OsString]) -> io::Result<Option<String>> {
    if arguments.len() != 3 {
        return Err(invalid("reporter requires three inherited handles"));
    }
    let mut handles = [HANDLE::default(); 3];
    for (argument, handle) in arguments.iter().zip(handles.iter_mut()) {
        let value = argument
            .to_str()
            .filter(|value| value.len() <= size_of::<usize>() * 2)
            .and_then(|value| usize::from_str_radix(value, 16).ok())
            .filter(|value| *value > 0 && *value <= isize::MAX as usize)
            .ok_or_else(|| invalid("invalid inherited reporter handle"))?;
        *handle = HANDLE(value as *mut _);
    }
    if handles[0] == handles[1] || handles[0] == handles[2] || handles[1] == handles[2] {
        return Err(invalid("reporter handles must be distinct"));
    }
    let mut in_job = false.into();
    unsafe { IsProcessInJob(GetCurrentProcess(), None, &mut in_job)? };
    if in_job.as_bool() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "reporter could not leave inherited process jobs",
        ));
    }
    for handle in handles {
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))? };
    }
    // This entry is terminal: its handles come only from the private child startup mode.
    let [mapping, parent, ready] = handles.map(owned);
    let view = View::open(raw(&mapping), FILE_MAP_WRITE)?;
    unsafe { SetEvent(raw(&ready))? };
    drop(ready);
    if unsafe { WaitForSingleObject(raw(&parent), INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let report = unsafe { &*view.address.Value.cast::<ReportRecord>() }.read();
    drop(view);
    drop(mapping);
    drop(parent);
    Ok(report)
}

fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
