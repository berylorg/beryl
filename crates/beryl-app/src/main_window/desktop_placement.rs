use beryl_model::VirtualDesktopId;
use gpui::WindowsHiddenWindowLease;
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_ALL, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoCreateInstance,
            CoInitializeEx, CoUninitialize,
        },
        UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager},
    },
    core::GUID,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDesktopPlacementStage {
    InitializeCom,
    CreateManager,
    MoveWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsDesktopPlacementFailure {
    pub stage: WindowsDesktopPlacementStage,
    pub hresult: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDesktopPlacementOutcome {
    SavedDesktopAccepted {
        desktop: VirtualDesktopId,
    },
    CurrentDesktopDefault {
        failure: Option<WindowsDesktopPlacementFailure>,
    },
}

pub fn windows_desktop_id_to_guid(desktop: VirtualDesktopId) -> GUID {
    GUID::from_u128(u128::from_be_bytes(*desktop.as_bytes()))
}

pub fn windows_desktop_id_from_guid(desktop: GUID) -> VirtualDesktopId {
    VirtualDesktopId::from_bytes(desktop.to_u128().to_be_bytes())
}

pub fn prepare_windows_desktop_placement(
    lease: WindowsHiddenWindowLease,
    saved: Option<VirtualDesktopId>,
) -> WindowsDesktopPlacementOutcome {
    let outcome = match saved {
        Some(desktop) => match move_to_saved_desktop(lease.raw_handle(), desktop) {
            Ok(()) => WindowsDesktopPlacementOutcome::SavedDesktopAccepted { desktop },
            Err(failure) => WindowsDesktopPlacementOutcome::CurrentDesktopDefault {
                failure: Some(failure),
            },
        },
        None => WindowsDesktopPlacementOutcome::CurrentDesktopDefault { failure: None },
    };
    drop(lease);
    outcome
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn move_to_saved_desktop(
    raw: usize,
    desktop: VirtualDesktopId,
) -> Result<(), WindowsDesktopPlacementFailure> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| WindowsDesktopPlacementFailure {
            stage: WindowsDesktopPlacementStage::InitializeCom,
            hresult: error.code().0,
        })?;
    let _apartment = ComApartment;
    let manager: IVirtualDesktopManager = unsafe {
        CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL)
    }
    .map_err(|error| WindowsDesktopPlacementFailure {
        stage: WindowsDesktopPlacementStage::CreateManager,
        hresult: error.code().0,
    })?;
    let desktop = windows_desktop_id_to_guid(desktop);
    unsafe { manager.MoveWindowToDesktop(HWND(raw as *mut _), &desktop) }.map_err(|error| {
        WindowsDesktopPlacementFailure {
            stage: WindowsDesktopPlacementStage::MoveWindow,
            hresult: error.code().0,
        }
    })
}
