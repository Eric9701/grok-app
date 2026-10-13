//! Let caption buttons receive clicks that land on a child WebView2.
//!
//! Octo and Atlas Cloud stay mounted as child webviews. Those HWNDs paint
//! above HTML, so the self-drawn minimize / maximize / close controls never
//! see the pointer once a child covers that corner — including when a bounds
//! update leaves the child at the window origin. CSS z-index cannot fix it.
//!
//! For every side-browser container (and the WebView2 windows it owns) we:
//! - return `HTTRANSPARENT` from `WM_NCHITTEST` inside the caption button band
//!   so the click falls through to the main webview on the same thread
//! - clip that band out of the container's window region so the buttons stay
//!   visible, and put the region back when the child no longer overlaps

#![cfg(windows)]

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, GetDC, GetDeviceCaps, MapWindowPoints, ReleaseDC,
    ScreenToClient, SetWindowRgn, HGDIOBJ, LOGPIXELSX, RGN_DIFF,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetAncestor, GetClientRect, KillTimer, SetTimer, SetWindowPos, GA_ROOT,
    HTTRANSPARENT, SWP_NOACTIVATE, SWP_NOZORDER, WM_CREATE, WM_NCDESTROY, WM_NCHITTEST,
    WM_PARENTNOTIFY, WM_TIMER, WM_WINDOWPOSCHANGED,
};

/// Matches `--titlebar-height` (40px) and `--window-controls-inset` (3 × 46px).
const CAPTION_HEIGHT_DIP: f64 = 40.0;
const CAPTION_BUTTONS_DIP: f64 = 138.0;
/// Extra DIPs so a subpixel gap does not leave the button edge dead.
const CAPTION_SLOP_DIP: f64 = 4.0;

const SUBCLASS_ID: usize = 0x4154_4350;
const TIMER_ID: usize = 0x4154_5452;
const SCAN_FOR: Duration = Duration::from_secs(2);

struct HoleState {
    width: i32,
    height: i32,
    hole: Option<(i32, i32, i32, i32)>,
}

thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn holes() -> std::sync::MutexGuard<'static, HashMap<isize, HoleState>> {
    HOLES.lock().unwrap_or_else(|e| e.into_inner())
}

fn scans() -> std::sync::MutexGuard<'static, HashMap<isize, Instant>> {
    SCAN_UNTIL.lock().unwrap_or_else(|e| e.into_inner())
}

static HOLES: LazyLock<Mutex<HashMap<isize, HoleState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SCAN_UNTIL: LazyLock<Mutex<HashMap<isize, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.set(false);
    }
}

fn hwnd_key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn dpi_of(hwnd: HWND) -> u32 {
    unsafe {
        let hdc = GetDC(Some(hwnd));
        if hdc.is_invalid() {
            return 96;
        }
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSX);
        let _ = ReleaseDC(Some(hwnd), hdc);
        if dpi <= 0 {
            96
        } else {
            dpi as u32
        }
    }
}

fn caption_band(root: HWND) -> Option<RECT> {
    let mut client = RECT::default();
    if unsafe { GetClientRect(root, &mut client) }.is_err() {
        return None;
    }
    if client.right <= 1 || client.bottom <= 1 {
        return None;
    }
    let scale = dpi_of(root) as f64 / 96.0;
    let band_h = ((CAPTION_HEIGHT_DIP + CAPTION_SLOP_DIP) * scale).ceil() as i32;
    let band_w = ((CAPTION_BUTTONS_DIP + CAPTION_SLOP_DIP) * scale).ceil() as i32;
    Some(RECT {
        left: client.right.saturating_sub(band_w),
        top: 0,
        right: client.right,
        bottom: band_h.min(client.bottom),
    })
}

fn root_of(hwnd: HWND) -> Option<HWND> {
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.0.is_null() || root == hwnd {
        None
    } else {
        Some(root)
    }
}

fn lparam_point(lp: LPARAM) -> (i32, i32) {
    let v = lp.0 as u32;
    let x = (v & 0xffff) as u16 as i16 as i32;
    let y = ((v >> 16) & 0xffff) as u16 as i16 as i32;
    (x, y)
}

fn point_in_caption(hwnd: HWND, lp: LPARAM) -> bool {
    let Some(root) = root_of(hwnd) else {
        return false;
    };
    let Some(band) = caption_band(root) else {
        return false;
    };
    let (x, y) = lparam_point(lp);
    let mut pt = POINT { x, y };
    if !unsafe { ScreenToClient(root, &mut pt) }.as_bool() {
        return false;
    }
    pt.x >= band.left && pt.x < band.right && pt.y >= band.top && pt.y < band.bottom
}

fn punch(hwnd: HWND) {
    if BUSY.get() {
        return;
    }
    BUSY.set(true);
    let _guard = BusyGuard;
    let Some(root) = root_of(hwnd) else {
        return;
    };
    let Some(band) = caption_band(root) else {
        return;
    };
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
        return;
    }
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width < 2 || height < 2 {
        return;
    }

    let mut corners = [
        POINT {
            x: band.left,
            y: band.top,
        },
        POINT {
            x: band.right,
            y: band.bottom,
        },
    ];
    unsafe { MapWindowPoints(Some(root), Some(hwnd), &mut corners) };
    let hx1 = corners[0].x.max(0);
    let hy1 = corners[0].y.max(0);
    let hx2 = corners[1].x.min(width);
    let hy2 = corners[1].y.min(height);
    let hole = if hx2 - hx1 >= 2 && hy2 - hy1 >= 2 {
        Some((hx1, hy1, hx2, hy2))
    } else {
        None
    };
    let next = HoleState {
        width,
        height,
        hole,
    };
    let key = hwnd_key(hwnd);
    if holes().get(&key).is_some_and(|prev| {
        prev.width == next.width && prev.height == next.height && prev.hole == next.hole
    }) {
        return;
    }

    unsafe {
        if let Some((x1, y1, x2, y2)) = hole {
            let full = CreateRectRgn(0, 0, width, height);
            let cut = CreateRectRgn(x1, y1, x2, y2);
            let dest = CreateRectRgn(0, 0, 0, 0);
            if full.is_invalid() || cut.is_invalid() || dest.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(full.0));
                let _ = DeleteObject(HGDIOBJ(cut.0));
                let _ = DeleteObject(HGDIOBJ(dest.0));
                return;
            }
            let mode = CombineRgn(Some(dest), Some(full), Some(cut), RGN_DIFF);
            let _ = DeleteObject(HGDIOBJ(full.0));
            let _ = DeleteObject(HGDIOBJ(cut.0));
            if mode.0 == 0 {
                let _ = DeleteObject(HGDIOBJ(dest.0));
                return;
            }
            // SetWindowRgn takes ownership of `dest`.
            if SetWindowRgn(hwnd, Some(dest), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(dest.0));
                return;
            }
        } else if holes().get(&key).is_some_and(|prev| prev.hole.is_some()) {
            let _ = SetWindowRgn(hwnd, None, true);
        } else {
            return;
        }
    }
    holes().insert(key, next);
}

fn attach(hwnd: HWND, root: bool) {
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = SetWindowSubclass(
            hwnd,
            Some(caption_proc),
            SUBCLASS_ID,
            if root { 1 } else { 0 },
        );
    }
}

unsafe extern "system" fn enum_child(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    attach(hwnd, false);
    true.into()
}

fn attach_tree(hwnd: HWND) {
    attach(hwnd, true);
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(enum_child), LPARAM(0));
    }
    punch(hwnd);
}

fn arm_scan(hwnd: HWND) {
    scans().insert(hwnd_key(hwnd), Instant::now() + SCAN_FOR);
    unsafe {
        let _ = SetTimer(Some(hwnd), TIMER_ID, 200, None);
    }
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    root_flag: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST && point_in_caption(hwnd, lparam) {
        return LRESULT(HTTRANSPARENT as isize);
    }

    if msg == WM_PARENTNOTIFY && root_flag == 1 {
        let event = (wparam.0 as u32) & 0xffff;
        if event == WM_CREATE {
            let child = HWND(lparam.0 as *mut core::ffi::c_void);
            if !child.0.is_null() {
                attach(child, false);
            }
        }
    }

    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(caption_proc), SUBCLASS_ID);
        let key = hwnd_key(hwnd);
        holes().remove(&key);
        scans().remove(&key);
        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    let result = DefSubclassProc(hwnd, msg, wparam, lparam);

    if root_flag == 1 && !BUSY.get() {
        if msg == WM_WINDOWPOSCHANGED {
            punch(hwnd);
        } else if msg == WM_TIMER && wparam.0 == TIMER_ID {
            attach_tree(hwnd);
            let key = hwnd_key(hwnd);
            let expired = scans()
                .get(&key)
                .map(|until| Instant::now() >= *until)
                .unwrap_or(true);
            if expired {
                scans().remove(&key);
                let _ = KillTimer(Some(hwnd), TIMER_ID);
            }
        }
    }

    result
}

/// Place the child and keep the caption button band clickable.
///
/// `x`/`y`/`width`/`height` are logical pixels (same as the DOM rect).
pub fn apply(controller: ICoreWebView2Controller, x: f64, y: f64, width: f64, height: f64) {
    let mut hwnd = HWND::default();
    if unsafe { controller.ParentWindow(&mut hwnd) }.is_err() || hwnd.0.is_null() {
        return;
    }
    let scale = dpi_of(hwnd) as f64 / 96.0;
    let px = (x * scale).round() as i32;
    let py = (y * scale).round() as i32;
    let pw = (width * scale).round().max(1.0) as i32;
    let ph = (height * scale).round().max(1.0) as i32;
    unsafe {
        let _ = SetWindowPos(hwnd, None, px, py, pw, ph, SWP_NOZORDER | SWP_NOACTIVATE);
        let _ = controller.SetBounds(RECT {
            left: 0,
            top: 0,
            right: pw,
            bottom: ph,
        });
        let _ = controller.NotifyParentWindowPositionChanged();
    }
    attach_tree(hwnd);
    arm_scan(hwnd);
}
