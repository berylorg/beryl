use std::{io, ptr};

use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
    UI::Input::KeyboardAndMouse::GetActiveWindow,
};

pub(super) fn copy(report: &str) -> io::Result<()> {
    let owner = unsafe { GetActiveWindow() };
    if owner.is_invalid() || report.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "report cannot be copied",
        ));
    }
    let text: Vec<u16> = report.encode_utf16().chain(Some(0)).collect();
    let mut memory =
        GlobalMemory(unsafe { GlobalAlloc(GMEM_MOVEABLE, text.len() * size_of::<u16>())? });
    let destination = unsafe { GlobalLock(memory.0) }.cast::<u16>();
    if destination.is_null() {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), destination, text.len());
    }
    // A zero lock count is successful GlobalUnlock and is also represented as an error.
    let _ = unsafe { GlobalUnlock(memory.0) };
    unsafe {
        OpenClipboard(Some(owner))?;
    }
    let clipboard = Clipboard;
    unsafe {
        EmptyClipboard()?;
        SetClipboardData(13, Some(HANDLE(memory.0.0)))?;
    }
    memory.0 = HGLOBAL::default();
    drop(clipboard);
    Ok(())
}

struct Clipboard;

impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

struct GlobalMemory(HGLOBAL);

impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { GlobalFree(Some(self.0)) };
        }
    }
}
