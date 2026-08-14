use std::{
    ffi::c_void,
    os::windows::ffi::OsStrExt,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use crate::{
    diagnostics::{DiagnosticAction, DiagnosticPresentation, DiagnosticPresentationKind},
    model::{self, Action, Area, OperationState, StatusKind},
};
use frametime_hardware_windows::WindowsHardwareDiagnostics;
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, GetSysColorBrush, HBRUSH, InvalidateRect,
            UpdateWindow,
        },
        System::{
            Console::{AttachConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent},
            LibraryLoader::GetModuleHandleW,
            Threading::{CREATE_NEW_CONSOLE, CREATE_NEW_PROCESS_GROUP},
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW, NotifyWinEvent},
            Controls::Dialogs::{GetSaveFileNameW, OFN_OVERWRITEPROMPT, OPENFILENAMEW},
            Controls::{
                BST_CHECKED, InitCommonControls, LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW, LVIF_TEXT,
                LVITEMW, LVM_DELETEALLITEMS, LVM_GETITEMTEXTW, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW,
                LVM_INSERTITEMW, LVM_SETITEMTEXTW, LVNI_SELECTED, LVS_REPORT, LVS_SHOWSELALWAYS,
                LVS_SINGLESEL,
            },
            HiDpi::{
                AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetDpiForWindow, SetProcessDpiAwarenessContext,
            },
            Input::KeyboardAndMouse::{
                EnableWindow, GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_F6,
            },
            Shell::{
                IsUserAnAdmin, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
                SHELLEXECUTEINFOW, ShellExecuteExW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

const NAV_BASE: usize = 100;
const ACTION: usize = 200;
const CANCEL: usize = 201;
const FPS_INPUT: usize = 202;
const MIN_INPUT: usize = 203;
const FOCUS_RESTORE: usize = 204;
const SECONDARY: usize = 205;
const TERTIARY: usize = 206;
const VPROF_INPUT: usize = 207;
const QUATERNARY: usize = 208;
const PROFILE: usize = 209;
const DRY_RUN: usize = 210;
const CATALOG_FILTER: usize = 211;
const VIDEO_ROOT: usize = 212;
const VIDEO_TIER: usize = 213;
const QUINARY: usize = 214;
const POLL_TIMER: usize = 1;
const WORK_DIR: &str = r"C:\FRAMETIME_CFG";

mod actions;
mod catalog;
mod controls;
mod layout;
mod navigation;
mod package;
mod runtime;
mod state;
mod video;
mod window_proc;

use actions::*;
use catalog::*;
use controls::*;
use layout::*;
use navigation::*;
use package::*;
use runtime::*;
pub(crate) use state::safe_mode_active;
use state::{AppState, NativeOperation, NativeWorkerResult, RetainedChild};
use video::*;
use window_proc::*;

pub fn run(
    package: model::PackageAuthentication<frametime_windows::AuthenticatedPackage>,
) -> windows::core::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None)? }.0);
    unsafe {
        InitCommonControls();
    }
    let class = w!("FrametimeCfgNativeGui");
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW)? };
    let window_class = WNDCLASSW {
        hCursor: cursor,
        hInstance: instance,
        lpszClassName: class,
        lpfnWndProc: Some(window_proc),
        hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut c_void),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&window_class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("frametime.cfg"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1180,
            760,
            None,
            None,
            Some(instance),
            None,
        )?
    };
    create_controls(window, instance, package)?;
    unsafe {
        let _ = ShowWindow(window, SW_SHOW);
        let _ = UpdateWindow(window);
    }
    let accelerators = accelerators()?;
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        if unsafe { TranslateAcceleratorW(window, accelerators, &message) } == 0
            && !unsafe { IsDialogMessageW(window, &message) }.as_bool()
        {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    unsafe {
        let _ = DestroyAcceleratorTable(accelerators);
    }
    Ok(())
}
