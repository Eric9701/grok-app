//! Keep the self-drawn title row above child WebView2 windows.
//!
//! Octo and Atlas Cloud stay mounted as child webviews. Those HWNDs paint
//! above HTML, so a child that reaches the top of the client area covers the
//! session title and the minimize / maximize / close controls. CSS z-index
//! cannot put that row back.
//!
//! For every side-browser container (and the WebView2 windows it owns) we:
//! - place the child below the title row
//! - return `HTTRANSPARENT` from `WM_NCHITTEST` inside that row so a click
//!   falls through to the main webview on the same thread
//! - clip that row out of the container's window region so the title and
//!   buttons stay visible, and put the region back when the child no longer
//!   overlaps it

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
    HTTRANSPARENT, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOZORDER, WM_CREATE, WM_NCDESTROY,
    WM_NCHITTEST, WM_PARENTNOTIFY, WM_TIMER, WM_WINDOWPOSCHANGED,
};

/// Matches `--titlebar-height` (40px). The whole row, not only the three
/// caption buttons: the session title sits in the same band.
const CAPTION_HEIGHT_DIP: f64 = 40.0;
/// Extra DIPs so a subpixel gap does not leave the title edge dead.
const CAPTION_SLOP_DIP: f64 = 2.0;

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
    use windows::Win32::UI::HiDpi::GetDpiForWindow;

    // Match wry's child placement (`GetDpiForWindow`). On a per-monitor v2
    // process, `GetDeviceCaps(LOGPIXELSX)` stays at 96 or the system DPI while
    // the window sits on a 125% / 150% monitor. Scaling the DOM rect with that
    // value rewrites the bounds wry just set and leaves a gap beside Octo and
    // Atlas Cloud. GDI is only the fallback when the window has no DPI yet.
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi > 0 {
            return dpi;
        }
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
    Some(RECT {
        left: 0,
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

/// Place the child under the title row and keep that row clickable.
///
/// `x`/`y`/`width`/`height` are logical pixels (same as the DOM rect).
/// The scale must be `GetDpiForWindow`, the same one wry used for `set_bounds`.
pub fn apply(controller: ICoreWebView2Controller, x: f64, y: f64, width: f64, height: f64) {
    let mut hwnd = HWND::default();
    if unsafe { controller.ParentWindow(&mut hwnd) }.is_err() || hwnd.0.is_null() {
        return;
    }
    let scale = dpi_of(hwnd) as f64 / 96.0;
    // A negative origin is the off-screen park. Do not scale it: logical
    // -32000 at 150% is -48000, and Windows wraps that 16-bit move back
    // into the parent so the child covers settings and other pages.
    let (px, py, pw, ph) = if x < 0.0 || y < 0.0 {
        (-30_000, -30_000, 1, 1)
    } else {
        let mut py = (y * scale).round() as i32;
        let mut ph = (height * scale).round().max(1.0) as i32;
        // Title row is self-drawn HTML. A child whose top lands inside it
        // covers the session title. Push the window down and keep its
        // bottom edge. Queued after wry's own async move so this one sticks.
        let band_h = ((CAPTION_HEIGHT_DIP + CAPTION_SLOP_DIP) * scale).ceil() as i32;
        if py < band_h {
            let shift = band_h - py;
            py = band_h;
            ph = (ph - shift).max(1);
        }
        (
            (x * scale).round() as i32,
            py,
            (width * scale).round().max(1.0) as i32,
            ph,
        )
    };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            px,
            py,
            pw,
            ph,
            SWP_ASYNCWINDOWPOS | SWP_NOZORDER | SWP_NOACTIVATE,
        );
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
