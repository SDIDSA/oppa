//! Criterion 1's platform caret references, both independent of the spike's
//! GetGlyphs/GetGlyphPlacements pipeline:
//!
//! - [`LayoutOracle`]: `IDWriteTextLayout` + `HitTestTextPosition` — the
//!   canonical DirectWrite caret API (the stack a DWrite-based native
//!   control would use for composition anchoring).
//! - [`EditControlOracle`]: a real Win32 EDIT control queried with
//!   `EM_POSFROMCHAR` / `EM_GETRECT` — the "native edit control with
//!   identical font/size/DPR" cross-check DESIGN §9.2 criterion 1 names.
//!   The EDIT control is GDI-based (hinted integer advances); the ±2 device
//!   px tolerance absorbs exactly that class of difference, which is part
//!   of what the criterion measures.
//!
//! Both report per-UTF-16-code-unit caret x positions relative to the text
//! origin. Coordinates live in the 96-dpi space the framework calls device
//! px at DPR 1; the EDIT control runs under an explicitly DPI-unaware
//! thread context so display scaling cannot shift its coordinates into a
//! different space.

use oppa::text::TextError;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_HIT_TEST_METRICS,
};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetDC, GetTextExtentPoint32W, GetTextMetricsW, ReleaseDC,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, FF_DONTCARE, OUT_DEFAULT_PRECIS,
    TEXTMETRICW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_GETRECT, EM_POSFROMCHAR};
use windows::Win32::UI::HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_UNAWARE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW, RegisterClassW,
    SendMessageW, TranslateMessage, CS_HREDRAW, CS_VREDRAW, MSG, PM_REMOVE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_SETFONT, WM_SETTEXT, WNDCLASSW, WS_CHILD, WS_VISIBLE,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn slice(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

// ---------------------------------------------------------------------------
// IDWriteTextLayout oracle
// ---------------------------------------------------------------------------

pub struct LayoutOracle {
    factory: IDWriteFactory,
    format: IDWriteTextFormat,
}

impl LayoutOracle {
    /// `size_px` is CSS px; the em handed to DirectWrite is `size_px * dpr`,
    /// the same `TextStyle::em_size` rule, so positions are device px.
    pub fn new(family: &str, size_px: f32, dpr: f32) -> windows::core::Result<Self> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let family_w = wide(family);
            let locale_w = wide("en-US");
            let format = factory.CreateTextFormat(
                PCWSTR(family_w.as_ptr()),
                None::<&windows::Win32::Graphics::DirectWrite::IDWriteFontCollection>,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size_px * dpr,
                PCWSTR(locale_w.as_ptr()),
            )?;
            Ok(Self { factory, format })
        }
    }

    /// Caret x (device px, text-origin-relative) for every UTF-16 unit index
    /// `0..=len` (index `len` is the trailing caret).
    pub fn caret_x_per_unit(&self, text: &str) -> Result<Vec<f32>, TextError> {
        unsafe {
            let text_w = slice(text);
            let layout: IDWriteTextLayout = self
                .factory
                .CreateTextLayout(&text_w, &self.format, 1.0e6, 1.0e6)
                .map_err(|e| TextError::Backend(e.to_string()))?;
            let mut out = Vec::with_capacity(text_w.len() + 1);
            for unit in 0..=(text_w.len() as u32) {
                let mut ox = 0.0f32;
                let mut oy = 0.0f32;
                let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                layout
                    .HitTestTextPosition(unit, false, &mut ox, &mut oy, &mut metrics)
                    .map_err(|e| TextError::Backend(e.to_string()))?;
                out.push(ox);
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// Win32 EDIT control oracle
// ---------------------------------------------------------------------------

pub struct EditControlOracle {
    host: HWND,
    edit: HWND,
    font: windows::Win32::Graphics::Gdi::HFONT,
    hdc: windows::Win32::Graphics::Gdi::HDC,
    format_rect: RECT,
}

unsafe extern "system" fn host_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

impl EditControlOracle {
    /// Invisible host window + EDIT child; the thread is switched to a
    /// DPI-unaware context first so the control's client coordinates are
    /// 96-dpi px — the framework's DPR-1 device-px space.
    pub fn new(size_px: f32) -> windows::core::Result<Self> {
        unsafe {
            let _prev = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE);
            let hmodule = GetModuleHandleW(None)?;
            let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);
            let class_name = wide("oppa_spike_host");
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(host_wndproc),
                hInstance: hinstance,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            let host = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(wide("").as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                64,
                64,
                None,
                None,
                Some(hinstance),
                None,
            )?;
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("EDIT"),
                windows::core::w!(""),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x80), // + ES_AUTOHSCROLL (0x80)
                0,
                0,
                520,
                32,
                Some(host),
                None,
                Some(hinstance),
                None,
            )?;
            let font = CreateFontW(
                -(size_px as i32),
                0,
                0,
                0,
                DWRITE_FONT_WEIGHT_NORMAL.0,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                FF_DONTCARE.0 as u32,
                PCWSTR(wide("Segoe UI").as_ptr()),
            );
            SendMessageW(
                edit,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(0)),
            );
            let hdc = GetDC(Some(edit));
            pump_a_few();
            let format_rect = RECT::default();
            SendMessageW(
                edit,
                EM_GETRECT,
                Some(WPARAM(0)),
                Some(LPARAM(&format_rect as *const RECT as isize)),
            );
            Ok(Self {
                host,
                edit,
                font,
                hdc,
                format_rect,
            })
        }
    }

    pub fn set_text(&self, text: &str) {
        unsafe {
            let w = wide(text);
            SendMessageW(
                self.edit,
                WM_SETTEXT,
                Some(WPARAM(0)),
                Some(LPARAM(w.as_ptr() as isize)),
            );
        }
    }

    /// Raw diagnostics for rig calibration.
    pub fn raw_pos(&self, unit: u32) -> i64 {
        unsafe {
            SendMessageW(
                self.edit,
                EM_POSFROMCHAR,
                Some(WPARAM(unit as usize)),
                Some(LPARAM(0)),
            )
            .0 as i64
        }
    }

    pub fn format_rect(&self) -> RECT {
        self.format_rect
    }

    /// Caret x (96-dpi px, text-origin-relative) for every UTF-16 unit index
    /// `0..=len`. `EM_POSFROMCHAR` covers `0..len`; the trailing entry is the
    /// GDI prefix width of the full text (the metric the control positions
    /// carets with internally).
    pub fn caret_x_per_unit(&self, text: &str) -> Vec<f32> {
        self.set_text(text);
        pump_a_few();
        let units = text.encode_utf16().count();
        let mut out = Vec::with_capacity(units + 1);
        for unit in 0..units {
            let pos = unsafe {
                SendMessageW(
                    self.edit,
                    EM_POSFROMCHAR,
                    Some(WPARAM(unit)),
                    Some(LPARAM(0)),
                )
            };
            out.push(self.rel_x(pos));
        }
        out.push(self.gdi_prefix_width(text, units));
        out
    }

    fn rel_x(&self, pos: LRESULT) -> f32 {
        let packed = pos.0;
        let x = (packed & 0xFFFF) as u16 as i16;
        x as f32 - self.format_rect.left as f32
    }

    fn gdi_prefix_width(&self, text: &str, units: usize) -> f32 {
        unsafe {
            use windows::Win32::Graphics::Gdi::SelectObject;
            let old = SelectObject(self.hdc, self.font.into());
            let s = slice(text);
            let mut size = SIZE::default();
            let ok = GetTextExtentPoint32W(self.hdc, &s[..units], &mut size);
            SelectObject(self.hdc, old);

            if ok.as_bool() {
                size.cx as f32
            } else {
                0.0
            }
        }
    }

    /// (ascent, descent) of the control's font, in its client px.
    pub fn font_metrics(&self) -> (i32, i32) {
        unsafe {
            let mut tm = TEXTMETRICW::default();
            let _ = GetTextMetricsW(self.hdc, &mut tm);
            (tm.tmAscent, tm.tmDescent)
        }
    }
}

impl Drop for EditControlOracle {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font.into());
            let _ = ReleaseDC(Some(self.edit), self.hdc);
            let _ = DestroyWindow(self.edit);
            let _ = DestroyWindow(self.host);
        }
    }
}

fn pump_a_few() {
    unsafe {
        let mut msg = MSG::default();
        for _ in 0..64 {
            if !PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
