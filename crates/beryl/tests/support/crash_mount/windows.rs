use std::{
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
};

use windows::{
    Win32::{
        Foundation::{FILETIME, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetProcessId, GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
                QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{
            FindWindowExW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
        },
    },
    core::{PWSTR, w},
};

pub struct Reporter {
    handle: OwnedHandle,
    pid: u32,
    created: u64,
    cleanup: bool,
}

pub fn children(parent: HANDLE, executable: &Path) -> Vec<Reporter> {
    assert_eq!(
        unsafe { WaitForSingleObject(parent, 0) },
        WAIT_TIMEOUT,
        "parent exited before child adoption"
    );
    let parent_pid = unsafe { GetProcessId(parent) };
    let parent_created = creation(parent);
    let snapshot = owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.unwrap());
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut available = unsafe { Process32FirstW(raw(&snapshot), &mut entry) }.is_ok();
    let mut reporters = Vec::new();
    let mut count = 0;
    while available {
        count += 1;
        assert!(count <= 32_768, "process snapshot exceeded bound");
        if entry.th32ParentProcessID == parent_pid {
            if let Ok(handle) = unsafe {
                OpenProcess(
                    PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                    false,
                    entry.th32ProcessID,
                )
            } {
                let handle = owned(handle);
                let mut image = [0u16; 32_768];
                let mut length = image.len() as u32;
                if unsafe {
                    QueryFullProcessImageNameW(
                        raw(&handle),
                        PROCESS_NAME_WIN32,
                        PWSTR(image.as_mut_ptr()),
                        &mut length,
                    )
                }
                .is_ok()
                {
                    let actual = std::path::PathBuf::from(String::from_utf16_lossy(
                        &image[..length as usize],
                    ));
                    let matches = actual
                        .canonicalize()
                        .unwrap()
                        .as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(
                            &executable
                                .canonicalize()
                                .unwrap()
                                .as_os_str()
                                .to_string_lossy(),
                        );
                    let created = creation(raw(&handle));
                    if matches
                        && created >= parent_created
                        && unsafe { GetProcessId(raw(&handle)) } == entry.th32ProcessID
                        && exact_live_child(raw(&handle), parent, entry.th32ProcessID, parent_pid)
                    {
                        reporters.push(Reporter {
                            handle,
                            pid: entry.th32ProcessID,
                            created,
                            cleanup: true,
                        });
                    }
                }
            }
        }
        available = unsafe { Process32NextW(raw(&snapshot), &mut entry) }.is_ok();
    }
    assert_eq!(
        unsafe { WaitForSingleObject(parent, 0) },
        WAIT_TIMEOUT,
        "parent exited during child adoption"
    );
    reporters
}

fn exact_live_child(candidate: HANDLE, parent: HANDLE, pid: u32, parent_pid: u32) -> bool {
    if unsafe { WaitForSingleObject(candidate, 0) } != WAIT_TIMEOUT
        || unsafe { WaitForSingleObject(parent, 0) } != WAIT_TIMEOUT
    {
        return false;
    }
    let snapshot = owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.unwrap());
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut available = unsafe { Process32FirstW(raw(&snapshot), &mut entry) }.is_ok();
    let mut count = 0;
    let mut matches = false;
    while available {
        count += 1;
        assert!(count <= 32_768, "identity snapshot exceeded bound");
        if entry.th32ProcessID == pid {
            matches = entry.th32ParentProcessID == parent_pid;
            break;
        }
        available = unsafe { Process32NextW(raw(&snapshot), &mut entry) }.is_ok();
    }
    matches
        && unsafe { WaitForSingleObject(candidate, 0) } == WAIT_TIMEOUT
        && unsafe { WaitForSingleObject(parent, 0) } == WAIT_TIMEOUT
}

impl Reporter {
    pub fn identity(&self) -> (u32, u64) {
        (self.pid, self.created)
    }

    pub fn observation_identity(mut self) -> (u32, u64) {
        self.cleanup = false;
        self.identity()
    }

    pub fn exited(&self) -> bool {
        (unsafe { WaitForSingleObject(raw(&self.handle), 0) }) == WAIT_OBJECT_0
    }

    pub fn window(&self) -> Option<HWND> {
        if self.exited() {
            return None;
        }
        assert_eq!(creation(raw(&self.handle)), self.created);
        let mut previous = HWND::default();
        for _ in 0..1024 {
            let Ok(window) = (unsafe {
                FindWindowExW(None, Some(previous), None, w!("Beryl — Internal error"))
            }) else {
                return None;
            };
            let mut owner = 0;
            unsafe {
                GetWindowThreadProcessId(window, Some(&mut owner));
            }
            if owner == self.pid {
                return Some(window);
            }
            previous = window;
        }
        panic!("window enumeration exceeded bound");
    }

    pub fn close(&self, window: HWND) {
        assert!(!self.exited());
        unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.unwrap();
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        let handle = raw(&self.handle);
        if self.cleanup && unsafe { WaitForSingleObject(handle, 0) } != WAIT_OBJECT_0 {
            let _ = unsafe { TerminateProcess(handle, 3) };
            if unsafe { WaitForSingleObject(handle, 5_000) } != WAIT_OBJECT_0 {
                eprintln!(
                    "exact reporter cleanup deadline elapsed for retained process {} created {}",
                    self.pid, self.created
                );
            }
        }
    }
}

fn creation(handle: HANDLE) -> u64 {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }.unwrap();
    (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
}

fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}
