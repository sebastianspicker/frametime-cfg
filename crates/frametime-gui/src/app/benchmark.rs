use super::*;
use model::fps_session::FpsStage;
use windows::Win32::System::SystemServices::{SS_CENTER, SS_CENTERIMAGE, SS_SUNKEN};

mod events;
mod placement;
mod presentation;

pub(super) use events::{command, input_changed, present_result};
pub(super) use placement::place;
pub(super) use presentation::{render, visibility};

const OPEN: usize = FPS_BASE;
const PASTE: usize = FPS_BASE + 1;
const NEXT: usize = FPS_BASE + 2;
const BACK: usize = FPS_BASE + 3;
const STRATEGY: usize = FPS_BASE + 4;
const REFRESH_RATE: usize = FPS_BASE + 5;
const MARGIN: usize = FPS_BASE + 6;
const LABEL: usize = FPS_BASE + 7;
const HISTORY: usize = FPS_BASE + 8;
const ETW: usize = FPS_BASE + 9;
const ELEVATE: usize = FPS_BASE + 10;

#[derive(Clone, Copy)]
pub(super) struct FpsControls {
    pub(in crate::app) steps: [HWND; 4],
    group_left: HWND,
    group_right: HWND,
    source: HWND,
    open: HWND,
    paste: HWND,
    next: HWND,
    back: HWND,
    strategy_label: HWND,
    strategy: HWND,
    refresh_label: HWND,
    refresh: HWND,
    margin_label: HWND,
    margin: HWND,
    label_label: HWND,
    label: HWND,
    summary: HWND,
    permission: HWND,
    history: HWND,
    etw: HWND,
    elevate: HWND,
}

impl FpsControls {
    fn handles(self) -> Vec<HWND> {
        let mut handles = self.steps.to_vec();
        handles.extend([
            self.group_left,
            self.group_right,
            self.source,
            self.open,
            self.paste,
            self.next,
            self.back,
            self.strategy_label,
            self.strategy,
            self.refresh_label,
            self.refresh,
            self.margin_label,
            self.margin,
            self.label_label,
            self.label,
            self.summary,
            self.permission,
            self.history,
            self.etw,
            self.elevate,
        ]);
        handles
    }
}

pub(super) fn create(parent: HWND, instance: HINSTANCE) -> windows::core::Result<FpsControls> {
    let group = |value: &str| {
        create_text_control(
            parent,
            instance,
            "BUTTON",
            value,
            WS_CHILD | WINDOW_STYLE(BS_GROUPBOX as u32),
            0,
        )
    };
    let group_left = group("Selected source")?;
    let group_right = group("Strategy and calculation")?;
    let text = |value: &str| create_static_text(parent, instance, value, 0);
    let button = |value, id| create_button(parent, instance, value, id);
    let edit = |value, id| {
        create_text_control(
            parent,
            instance,
            "EDIT",
            value,
            WS_CHILD | WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            id,
        )
    };
    let strategy_label = text("&Strategy")?;
    let strategy = create_text_control(
        parent,
        instance,
        "COMBOBOX",
        "",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        STRATEGY,
    )?;
    for label in ["Raw (0 = uncapped)", "VRR ceiling"] {
        let label = utf16(label);
        unsafe {
            SendMessageW(
                strategy,
                CB_ADDSTRING,
                None,
                Some(LPARAM(label.as_ptr() as isize)),
            );
        }
    }
    unsafe {
        SendMessageW(strategy, CB_SETCURSEL, Some(WPARAM(0)), None);
    }
    // Inputs start without fabricated monitor data. Raw remains the existing default.
    let controls = FpsControls {
        group_left,
        group_right,
        steps: [
            text("1  Import")?,
            text("2  Evaluate")?,
            text("3  Review")?,
            text("4  Save")?,
        ],
        source: text("Selected source: pasted text")?,
        open: button("Choose &file...", OPEN)?,
        paste: button("Paste &text instead", PASTE)?,
        next: button("&Evaluate capture >", NEXT)?,
        back: button("< &Back", BACK)?,
        strategy_label,
        strategy,
        refresh_label: text("&Refresh rate (Hz)")?,
        refresh: edit("", REFRESH_RATE)?,
        margin_label: text("&Margin (Hz)")?,
        margin: edit("3", MARGIN)?,
        label_label: text("Capture &label")?,
        label: edit("CS2 capture", LABEL)?,
        summary: create_readout(parent, instance)?,
        permission: create_readout(parent, instance)?,
        history: button("&History / diagnostics", HISTORY)?,
        etw: button("Capture 5s ETW frames", ETW)?,
        elevate: button("Open as administrator", ELEVATE)?,
    };
    for step in controls.steps {
        unsafe {
            SetWindowLongW(
                step,
                GWL_STYLE,
                (WS_CHILD.0 | WS_VISIBLE.0 | SS_CENTER.0 | SS_SUNKEN.0 | SS_CENTERIMAGE.0) as i32,
            );
        }
    }
    unsafe {
        SendMessageW(controls.label, EM_LIMITTEXT, Some(WPARAM(128)), None);
    }
    Ok(controls)
}

fn create_readout(parent: HWND, instance: HINSTANCE) -> windows::core::Result<HWND> {
    create_text_control(
        parent,
        instance,
        "EDIT",
        "",
        WS_CHILD
            | WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
        0,
    )
}
