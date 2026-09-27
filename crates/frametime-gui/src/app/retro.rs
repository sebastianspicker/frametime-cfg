//! Windows 98-inspired presentation for the native Win32 frontend.
//!
//! This module changes only per-window GDI resources and child-window themes.
//! The controls remain native HWNDs, preserving their keyboard and accessibility
//! behavior, and high contrast always uses the active system colors.

use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BF_RECT, BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_HIGHLIGHT,
            COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT, CreateFontW, CreateSolidBrush,
            DEFAULT_CHARSET, DEFAULT_PITCH, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER,
            DeleteObject, DrawEdge, DrawTextW, EDGE_RAISED, EndPaint, FF_DONTCARE, FW_BOLD,
            FW_NORMAL, FillRect, GetSysColor, GetSysColorBrush, HBRUSH, HDC, HFONT, HGDIOBJ,
            OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkColor, SetBkMode, SetTextColor,
            TRANSPARENT,
        },
        UI::{
            Controls::SetWindowTheme,
            WindowsAndMessaging::{
                EnumChildWindows, GetClassNameW, GetClientRect, SendMessageW, WM_SETFONT,
            },
        },
    },
    core::{BOOL, Error, PCWSTR, Result, w},
};

const LOGICAL_DPI: u32 = 96;
const TITLE_HEIGHT: i32 = 32;
const CONTENT_GAP: i32 = 4;
const FACE_COLOR: COLORREF = COLORREF(0x00C0_C0C0);
const TITLE_COLOR: COLORREF = COLORREF(0x0080_0000);
const TITLE_TEXT_COLOR: COLORREF = COLORREF(0x00FF_FFFF);

/// Per-window resources for the classic visual treatment.
///
/// Keep this value in `AppState`: controls retain the font handles after
/// `WM_SETFONT`, so the fonts must outlive every child HWND that uses them.
pub(super) struct RetroResources {
    face_brush: OwnedBrush,
    title_brush: OwnedBrush,
    fonts: Fonts,
}

impl RetroResources {
    pub(super) fn new(
        parent: HWND,
        heading: HWND,
        status: HWND,
        dpi: u32,
        high_contrast: bool,
    ) -> Result<Self> {
        let resources = Self {
            face_brush: OwnedBrush::new(FACE_COLOR)?,
            title_brush: OwnedBrush::new(TITLE_COLOR)?,
            fonts: Fonts::new(dpi)?,
        };
        resources.apply(parent, heading, status, high_contrast);
        Ok(resources)
    }

    /// Rebuild DPI-dependent fonts and reapply the local theme preference.
    pub(super) fn refresh(
        &mut self,
        parent: HWND,
        heading: HWND,
        status: HWND,
        dpi: u32,
        high_contrast: bool,
    ) -> Result<()> {
        let replacement = Fonts::new(dpi)?;
        let previous = std::mem::replace(&mut self.fonts, replacement);
        self.apply(parent, heading, status, high_contrast);
        drop(previous);
        Ok(())
    }

    /// Paint the client face, internal title strip, and classic outer edge.
    pub(super) fn paint_client(
        &self,
        window: HWND,
        task_title: &str,
        high_contrast: bool,
    ) -> LRESULT {
        let mut paint = PAINTSTRUCT::default();
        let hdc = unsafe { BeginPaint(window, &mut paint) };
        let mut client = RECT::default();
        if unsafe { GetClientRect(window, &mut client) }.is_ok() {
            let face = if high_contrast {
                unsafe { GetSysColorBrush(COLOR_WINDOW) }
            } else {
                self.face_brush.handle()
            };
            unsafe {
                let _ = FillRect(hdc, &client, face);
            }
            self.paint_title(hdc, client, task_title, high_contrast);
            paint_outer_edge(hdc, client);
        }
        unsafe {
            let _ = EndPaint(window, &paint);
        }
        LRESULT(0)
    }

    /// Handle `WM_CTLCOLORSTATIC` while retaining the app's status-kind color.
    ///
    /// The caller supplies its existing status color so Ready/Running/Warning/
    /// Failed semantics stay under the application's established policy.
    pub(super) fn handle_static_color(
        &self,
        hdc: HDC,
        target: HWND,
        heading: HWND,
        status: HWND,
        status_color: COLORREF,
        high_contrast: bool,
    ) -> Option<LRESULT> {
        let (text, background, brush) = if target == heading {
            if high_contrast {
                system_colors(COLOR_WINDOWTEXT, COLOR_WINDOW)
            } else {
                (
                    COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) }),
                    FACE_COLOR,
                    self.face_brush.handle(),
                )
            }
        } else if target == status {
            if high_contrast {
                system_colors(COLOR_WINDOWTEXT, COLOR_WINDOW)
            } else {
                (status_color, FACE_COLOR, self.face_brush.handle())
            }
        } else if high_contrast || is_edit_control(target) {
            system_colors(COLOR_WINDOWTEXT, COLOR_WINDOW)
        } else {
            (
                COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) }),
                FACE_COLOR,
                self.face_brush.handle(),
            )
        };
        unsafe {
            let _ = SetTextColor(hdc, text);
            let _ = SetBkColor(hdc, background);
        }
        Some(LRESULT(brush.0 as isize))
    }

    /// Color an FPS workflow step label while leaving it a native STATIC control.
    pub(super) fn color_step(&self, hdc: HDC, active: bool, high_contrast: bool) -> LRESULT {
        let (text, background, brush) = match (active, high_contrast) {
            (true, true) => system_colors(COLOR_HIGHLIGHTTEXT, COLOR_HIGHLIGHT),
            (false, true) => system_colors(COLOR_WINDOWTEXT, COLOR_WINDOW),
            (true, false) => (TITLE_TEXT_COLOR, TITLE_COLOR, self.title_brush.handle()),
            (false, false) => (
                COLORREF(unsafe { GetSysColor(COLOR_WINDOWTEXT) }),
                FACE_COLOR,
                self.face_brush.handle(),
            ),
        };
        unsafe {
            let _ = SetTextColor(hdc, text);
            let _ = SetBkColor(hdc, background);
        }
        LRESULT(brush.0 as isize)
    }

    fn apply(&self, parent: HWND, heading: HWND, status: HWND, high_contrast: bool) {
        let context = ChildStyleContext {
            body: self.fonts.body.handle(),
            high_contrast,
        };
        unsafe {
            let _ = EnumChildWindows(
                Some(parent),
                Some(style_child),
                LPARAM(std::ptr::from_ref(&context) as isize),
            );
            set_font(heading, self.fonts.heading.handle());
            set_font(status, self.fonts.emphasis.handle());
        }
    }

    fn paint_title(&self, hdc: HDC, client: RECT, task_title: &str, high_contrast: bool) {
        let mut title = RECT {
            bottom: scale(self.fonts.dpi, TITLE_HEIGHT),
            ..client
        };
        let brush = if high_contrast {
            unsafe { GetSysColorBrush(COLOR_HIGHLIGHT) }
        } else {
            self.title_brush.handle()
        };
        unsafe {
            let _ = FillRect(hdc, &title, brush);
        }
        title.left += scale(self.fonts.dpi, 8);
        title.right -= scale(self.fonts.dpi, 8);
        let mut text = format!("frametime.cfg - {task_title}")
            .encode_utf16()
            .collect::<Vec<_>>();
        let text_color = if high_contrast {
            COLORREF(unsafe { GetSysColor(COLOR_HIGHLIGHTTEXT) })
        } else {
            TITLE_TEXT_COLOR
        };
        unsafe {
            let previous = SelectObject(hdc, HGDIOBJ::from(self.fonts.title.handle()));
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, text_color);
            let _ = DrawTextW(
                hdc,
                &mut text,
                &mut title,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
            let _ = SelectObject(hdc, previous);
        }
    }
}

/// Height reserved for the internal task-title strip.
pub(super) const fn banner_height(dpi: u32) -> i32 {
    scale(dpi, TITLE_HEIGHT)
}

/// Offset to add to existing content layout coordinates.
pub(super) const fn content_offset(dpi: u32) -> i32 {
    banner_height(dpi) + scale(dpi, CONTENT_GAP)
}

struct Fonts {
    dpi: u32,
    body: OwnedFont,
    title: OwnedFont,
    heading: OwnedFont,
    emphasis: OwnedFont,
}

impl Fonts {
    fn new(dpi: u32) -> Result<Self> {
        let dpi = dpi.max(LOGICAL_DPI);
        Ok(Self {
            dpi,
            body: OwnedFont::new(dpi, 9, FW_NORMAL)?,
            title: OwnedFont::new(dpi, 9, FW_BOLD)?,
            heading: OwnedFont::new(dpi, 15, FW_BOLD)?,
            emphasis: OwnedFont::new(dpi, 9, FW_NORMAL)?,
        })
    }
}

struct OwnedBrush(HBRUSH);

impl OwnedBrush {
    fn new(color: COLORREF) -> Result<Self> {
        let handle = unsafe { CreateSolidBrush(color) };
        if handle.0.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(handle))
        }
    }

    const fn handle(&self) -> HBRUSH {
        self.0
    }
}

impl Drop for OwnedBrush {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ::from(self.0));
        }
    }
}

struct OwnedFont(HFONT);

impl OwnedFont {
    fn new(
        dpi: u32,
        points: i32,
        weight: windows::Win32::Graphics::Gdi::FONT_WEIGHT,
    ) -> Result<Self> {
        let height = -((i32::try_from(dpi).expect("supported DPI fits in i32") * points) / 72);
        let pitch_and_family = u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0);
        let handle = unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                weight.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                pitch_and_family,
                w!("Tahoma"),
            )
        };
        if handle.0.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self(handle))
        }
    }

    const fn handle(&self) -> HFONT {
        self.0
    }
}

impl Drop for OwnedFont {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ::from(self.0));
        }
    }
}

struct ChildStyleContext {
    body: HFONT,
    high_contrast: bool,
}

unsafe extern "system" fn style_child(child: HWND, data: LPARAM) -> BOOL {
    let context = unsafe { &*(data.0 as *const ChildStyleContext) };
    unsafe {
        set_font(child, context.body);
        if context.high_contrast {
            let _ = SetWindowTheme(child, PCWSTR::null(), PCWSTR::null());
        } else {
            let _ = SetWindowTheme(child, w!(""), w!(""));
        }
    }
    BOOL(1)
}

unsafe fn set_font(window: HWND, font: HFONT) {
    unsafe {
        SendMessageW(
            window,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

fn paint_outer_edge(hdc: HDC, client: RECT) {
    let mut edge = client;
    unsafe {
        let _ = DrawEdge(hdc, &mut edge, EDGE_RAISED, BF_RECT);
    }
}

fn is_edit_control(window: HWND) -> bool {
    let mut class_name = [0_u16; 8];
    let length = unsafe { GetClassNameW(window, &mut class_name) };
    length == 4
        && ascii_case_equal(class_name[0], b'E')
        && ascii_case_equal(class_name[1], b'D')
        && ascii_case_equal(class_name[2], b'I')
        && ascii_case_equal(class_name[3], b'T')
}

fn ascii_case_equal(actual: u16, uppercase: u8) -> bool {
    let uppercase = u16::from(uppercase);
    actual == uppercase || actual == uppercase + u16::from(b'a' - b'A')
}

fn system_colors(
    text_index: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX,
    background_index: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX,
) -> (COLORREF, COLORREF, HBRUSH) {
    unsafe {
        (
            COLORREF(GetSysColor(text_index)),
            COLORREF(GetSysColor(background_index)),
            GetSysColorBrush(background_index),
        )
    }
}

const fn scale(dpi: u32, value: i32) -> i32 {
    let dpi = if dpi < LOGICAL_DPI { LOGICAL_DPI } else { dpi };
    (value * dpi as i32) / LOGICAL_DPI as i32
}
