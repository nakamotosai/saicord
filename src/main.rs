#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod rec;
mod util;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SetFocus, UnregisterHotKey, MOD_NOREPEAT, VK_F9,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLS_OVERLAY: PCWSTR = w!("SaicordOverlay");
const CLS_BAR: PCWSTR = w!("SaicordBar");
const CLS_CHIP: PCWSTR = w!("SaicordChip");
const CLS_FRAME: PCWSTR = w!("SaicordFrame");
const CLS_TOAST: PCWSTR = w!("SaicordToast");

/// 未选中区域：半透明黑（0xAARRGGBB）
const DIM: u32 = 0x6600_0000;
/// 选区边框 / 录制框：琥珀 #FF9A4D
const ACCENT: u32 = 0xFF_FF_9A_4D;

const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}
const C_BG: COLORREF = rgb(0x1C, 0x1C, 0x1E);
const C_TEXT: COLORREF = rgb(0xFF, 0xFF, 0xFF);
const C_MUTED: COLORREF = rgb(0x9A, 0x9A, 0xA0);
const C_ACCENT: COLORREF = rgb(0xFF, 0x9A, 0x4D);
const C_HOVER: COLORREF = rgb(0x2E, 0x2E, 0x32);

struct Dib {
    hdc: HDC,
    ptr: *mut u32,
    w: i32,
    h: i32,
}

struct App {
    hinst: HINSTANCE,
    overlay: HWND,
    bar: HWND,
    chip: HWND,
    toast: HWND,
    frames: Vec<HWND>,
    hfont: HFONT,
    hfont_bold: HFONT,
    scale: f32,
    mon: RECT,
    mon_index: usize,
    sel: Option<RECT>,
    drag: Option<POINT>,
    dib: Option<Dib>,
    audio_on: bool,
    audio_dev: Option<audio::AudioInfo>,
    recording: bool,
    started_at: Instant,
    output: Option<String>,
    ctl: Option<rec::Control>,
    audio_stop: Arc<AtomicBool>,
    audio_thread: Option<std::thread::JoinHandle<()>>,
    first_frame: Arc<AtomicBool>,
    bar_hover: i32,
}

static mut APP: Option<App> = None;

#[allow(static_mut_refs)]
fn app() -> &'static mut App {
    unsafe { APP.as_mut().expect("app not initialised") }
}

/// 逻辑像素 → 物理像素（按当前显示器缩放）
fn s(v: i32) -> i32 {
    let a = app();
    (v as f32 * a.scale).round() as i32
}

fn bar_w() -> i32 {
    s(236)
}
fn bar_h() -> i32 {
    s(48)
}

pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(r"F:\tmp\saicord.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let hmodule = unsafe { GetModuleHandleW(None) }.expect("module handle");
    let hinst = HINSTANCE(hmodule.0);

    unsafe {
        APP = Some(App {
            hinst,
            overlay: HWND::default(),
            bar: HWND::default(),
            chip: HWND::default(),
            toast: HWND::default(),
            frames: Vec::new(),
            hfont: HFONT::default(),
            hfont_bold: HFONT::default(),
            scale: 1.0,
            mon: RECT::default(),
            mon_index: 1,
            sel: None,
            drag: None,
            dib: None,
            audio_on: true,
            audio_dev: audio::probe(),
            recording: false,
            started_at: Instant::now(),
            output: None,
            ctl: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
            audio_thread: None,
            first_frame: Arc::new(AtomicBool::new(false)),
            bar_hover: 0,
        });
        register_classes(app().hinst);
        open_overlay();
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn register_classes(hinst: HINSTANCE) {
    let classes: [(PCWSTR, WNDPROC); 5] = [
        (CLS_OVERLAY, Some(overlay_proc)),
        (CLS_BAR, Some(bar_proc)),
        (CLS_CHIP, Some(chip_proc)),
        (CLS_FRAME, Some(frame_proc)),
        (CLS_TOAST, Some(toast_proc)),
    ];
    for (name, proc) in classes {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: proc,
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

fn cursor_point() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

unsafe fn monitor_under_cursor() -> (RECT, String) {
    let p = cursor_point();
    let hmon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let _ = GetMonitorInfoW(hmon, &mut mi as *mut _ as *mut MONITORINFO);
    let name = String::from_utf16_lossy(&mi.szDevice)
        .trim_end_matches('\0')
        .to_string();
    (mi.monitorInfo.rcMonitor, name)
}

fn monitor_index_for(device: &str) -> usize {
    device
        .rsplit("DISPLAY")
        .next()
        .and_then(|n| n.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

unsafe fn make_fonts() {
    let a = app();
    let px = s(15);
    a.hfont = CreateFontW(
        -px, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH.0 as u32, w!("Microsoft YaHei UI"),
    );
    a.hfont_bold = CreateFontW(
        -px, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY, DEFAULT_PITCH.0 as u32, w!("Microsoft YaHei UI"),
    );
}

unsafe fn open_overlay() {
    let (mon, name) = monitor_under_cursor();
    let idx = monitor_index_for(&name);
    let w = mon.right - mon.left;
    let h = mon.bottom - mon.top;

    let a = app();
    a.mon = mon;
    a.mon_index = idx;
    a.sel = None;
    a.drag = None;

    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut ptr, None, 0).expect("dib");
    let _ = SelectObject(mem, HGDIOBJ(bmp.0));
    ReleaseDC(None, screen);

    a.dib = Some(Dib {
        hdc: mem,
        ptr: ptr as *mut u32,
        w,
        h,
    });

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        CLS_OVERLAY,
        w!(""),
        WS_POPUP,
        mon.left,
        mon.top,
        w,
        h,
        None,
        None,
        Some(a.hinst),
        None,
    )
    .expect("overlay window");
    a.overlay = hwnd;

    // 按这台显示器的缩放比缩放整套 UI
    let dpi = GetDpiForWindow(hwnd);
    a.scale = if dpi >= 96 { dpi as f32 / 96.0 } else { 1.0 };
    make_fonts();

    paint_overlay();
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(hwnd));
}

unsafe fn paint_overlay() {
    let a = app();
    let Some(dib) = a.dib.as_ref() else { return };
    let (w, h) = (dib.w, dib.h);
    let px = std::slice::from_raw_parts_mut(dib.ptr, (w * h) as usize);
    px.fill(DIM);

    if let Some(sel) = a.sel {
        let l = (sel.left - a.mon.left).clamp(0, w);
        let t = (sel.top - a.mon.top).clamp(0, h);
        let r = (sel.right - a.mon.left).clamp(0, w);
        let b = (sel.bottom - a.mon.top).clamp(0, h);
        if r > l && b > t {
            for y in t..b {
                let row = (y * w) as usize;
                px[row + l as usize..row + r as usize].fill(0);
            }
            let edge = (2.0 * a.scale).round() as i32;
            for y in t..b {
                let row = (y * w) as usize;
                let (a0, a1) = (l.max(0) as usize, (l + edge).min(r) as usize);
                if a1 > a0 {
                    px[row + a0..row + a1].fill(ACCENT);
                }
                let (b0, b1) = ((r - edge).max(l) as usize, r as usize);
                if b1 > b0 {
                    px[row + b0..row + b1].fill(ACCENT);
                }
            }
            for y in [t, (b - edge).max(t)] {
                for yy in y..(y + edge).min(b) {
                    let row = (yy * w) as usize;
                    px[row + l as usize..row + r as usize].fill(ACCENT);
                }
            }
        }
    }

    let screen = GetDC(None);
    let pt_dst = POINT {
        x: a.mon.left,
        y: a.mon.top,
    };
    let size = SIZE { cx: w, cy: h };
    let pt_src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let ulw = UpdateLayeredWindow(
        a.overlay,
        Some(screen),
        Some(&pt_dst),
        Some(&size),
        Some(dib.hdc),
        Some(&pt_src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    if let Err(e) = ulw {
        log(&format!("UpdateLayeredWindow failed: {e}"));
    }
    ReleaseDC(None, screen);
}

fn lparam_point(l: LPARAM) -> POINT {
    let x = (l.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((l.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    POINT { x, y }
}

unsafe fn draw_label(hdc: HDC, text: &str, rect: &mut RECT, bold: bool, color: COLORREF) {
    let a = app();
    let font = if bold { a.hfont_bold } else { a.hfont };
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);
    let mut buf = util::wide(text);
    DrawTextW(hdc, &mut buf, rect, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
    SelectObject(hdc, old);
}

unsafe fn fill(hdc: HDC, rect: &RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn round_window(hwnd: HWND, w: i32, h: i32, r: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r);
    SetWindowRgn(hwnd, Some(rgn), true);
}

// ---------------------------------------------------------------- overlay

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let p = lparam_point(lp);
            let a = app();
            let screen = POINT {
                x: a.mon.left + p.x,
                y: a.mon.top + p.y,
            };
            a.drag = Some(screen);
            a.sel = Some(RECT {
                left: screen.x,
                top: screen.y,
                right: screen.x + 1,
                bottom: screen.y + 1,
            });
            paint_overlay();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = lparam_point(lp);
            let a = app();
            if let Some(origin) = a.drag {
                let screen = POINT {
                    x: a.mon.left + p.x,
                    y: a.mon.top + p.y,
                };
                a.sel = Some(RECT {
                    left: origin.x.min(screen.x),
                    top: origin.y.min(screen.y),
                    right: origin.x.max(screen.x),
                    bottom: origin.y.max(screen.y),
                });
                paint_overlay();
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let a = app();
            a.drag = None;
            let min = s(32);
            let ok = a
                .sel
                .map(|sel| (sel.right - sel.left) >= min && (sel.bottom - sel.top) >= min)
                .unwrap_or(false);
            if ok {
                let _ = ShowWindow(hwnd, SW_HIDE);
                show_bar();
            } else {
                a.sel = None;
                paint_overlay();
            }
            LRESULT(0)
        }
        WM_RBUTTONUP | WM_MBUTTONUP => {
            std::process::exit(0);
        }
        WM_KEYDOWN => {
            if wp.0 == 0x1B {
                std::process::exit(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- toolbar

unsafe fn show_bar() {
    let a = app();
    let Some(sel) = a.sel else { return };
    let (bw, bh) = (bar_w(), bar_h());
    let mut x = sel.right - bw;
    let mut y = sel.bottom + s(12);
    if y + bh > a.mon.bottom {
        y = sel.bottom - bh - s(12);
    }
    if y < a.mon.top {
        y = sel.top + s(12);
    }
    if x < a.mon.left {
        x = a.mon.left + s(8);
    }

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        CLS_BAR,
        w!(""),
        WS_POPUP,
        x,
        y,
        bw,
        bh,
        None,
        None,
        Some(a.hinst),
        None,
    )
    .expect("bar");
    a.bar = hwnd;
    round_window(hwnd, bw, bh, s(12));
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let (bw, bh) = (bar_w(), bar_h());
            let full = RECT {
                left: 0,
                top: 0,
                right: bw,
                bottom: bh,
            };
            fill(hdc, &full, C_BG);

            let a = app();
            let pad = s(6);
            let start_r = s(96);
            let mic_l = s(100);
            let mic_r = s(196);
            let close_l = s(200);

            let mut r = RECT {
                left: s(8),
                top: pad,
                right: start_r,
                bottom: bh - pad,
            };
            if a.bar_hover == 1 {
                fill(hdc, &r, C_HOVER);
            }
            draw_label(hdc, "● 开始", &mut r, true, C_ACCENT);

            let mut r = RECT {
                left: mic_l,
                top: pad,
                right: mic_r,
                bottom: bh - pad,
            };
            if a.bar_hover == 2 {
                fill(hdc, &r, C_HOVER);
            }
            let mic_text = if a.audio_dev.is_none() {
                "无音频设备"
            } else if a.audio_on {
                "系统声 开"
            } else {
                "系统声 关"
            };
            draw_label(hdc, mic_text, &mut r, false, C_MUTED);

            let mut r = RECT {
                left: close_l,
                top: pad,
                right: bw - s(8),
                bottom: bh - pad,
            };
            if a.bar_hover == 3 {
                fill(hdc, &r, C_HOVER);
            }
            draw_label(hdc, "✕", &mut r, false, C_MUTED);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = lparam_point(lp);
            let zone = if (s(8)..s(96)).contains(&p.x) {
                1
            } else if (s(100)..s(196)).contains(&p.x) {
                2
            } else if (s(200)..bar_w() - s(8)).contains(&p.x) {
                3
            } else {
                0
            };
            let a = app();
            if a.bar_hover != zone {
                a.bar_hover = zone;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let p = lparam_point(lp);
            let a = app();
            if (s(8)..s(96)).contains(&p.x) {
                begin_record();
            } else if (s(100)..s(196)).contains(&p.x) {
                a.audio_on = !a.audio_on;
                let _ = InvalidateRect(Some(hwnd), None, false);
            } else if (s(200)..bar_w() - s(8)).contains(&p.x) {
                std::process::exit(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- recording

unsafe fn begin_record() {
    let a = app();
    let Some(sel) = a.sel else { return };
    let _ = DestroyWindow(a.bar);
    a.bar = HWND::default();

    let crop = (
        (sel.left - a.mon.left) as u32,
        (sel.top - a.mon.top) as u32,
        (sel.right - a.mon.left) as u32,
        (sel.bottom - a.mon.top) as u32,
    );

    let out = util::desktop_dir()
        .join(format!("录屏_{}.mp4", util::now_stamp()))
        .to_string_lossy()
        .to_string();

    let audio_cfg = if a.audio_on {
        a.audio_dev.map(|m| (m.sample_rate, m.channels))
    } else {
        None
    };
    a.first_frame = Arc::new(AtomicBool::new(false));

    match rec::start(a.mon_index, crop, out.clone(), audio_cfg, a.first_frame.clone()) {
        Ok(ctl) => {
            let cb = ctl.callback();
            a.ctl = Some(ctl);
            a.output = Some(out);
            a.recording = true;
            a.started_at = Instant::now();
            a.audio_stop = Arc::new(AtomicBool::new(false));

            if let Some(info) = a.audio_dev.filter(|_| a.audio_on) {
                let handle = audio::spawn(info, cb, a.first_frame.clone(), a.audio_stop.clone());
                a.audio_thread = Some(handle);
            }

            create_frame(sel);
            create_chip(sel);
            let _ = RegisterHotKey(Some(a.chip), 1, MOD_NOREPEAT, VK_F9.0 as u32);
        }
        Err(e) => {
            log(&format!("rec start failed: {e}"));
            show_toast(&format!("启动录制失败：{e}"));
        }
    }
}

unsafe fn create_frame(sel: RECT) {
    let a = app();
    let t = s(3);
    let rects = [
        (sel.left - t, sel.top - t, sel.right - sel.left + t * 2, t),
        (sel.left - t, sel.bottom, sel.right - sel.left + t * 2, t),
        (sel.left - t, sel.top, t, sel.bottom - sel.top),
        (sel.right, sel.top, t, sel.bottom - sel.top),
    ];
    for (x, y, w, h) in rects {
        if let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            CLS_FRAME,
            w!(""),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(a.hinst),
            None,
        ) {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            a.frames.push(hwnd);
        }
    }
}

unsafe fn create_chip(sel: RECT) {
    let a = app();
    let w = s(208);
    let h = s(38);
    let mut y = sel.top - h - s(10);
    if y < a.mon.top {
        y = sel.bottom + s(10);
    }
    let x = sel.left;
    if let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        CLS_CHIP,
        w!(""),
        WS_POPUP,
        x,
        y,
        w,
        h,
        None,
        None,
        Some(a.hinst),
        None,
    ) {
        a.chip = hwnd;
        round_window(hwnd, w, h, s(10));
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), 1, 250, None);
    }
}

unsafe fn stop_record() {
    let a = app();
    if !a.recording {
        return;
    }
    a.recording = false;
    let _ = UnregisterHotKey(Some(a.chip), 1);

    a.audio_stop.store(true, Ordering::Relaxed);
    if let Some(handle) = a.audio_thread.take() {
        let _ = handle.join();
    }

    if let Some(ctl) = a.ctl.take() {
        let cb = ctl.callback();
        let _ = ctl.stop();
        let mut rec = cb.lock();
        if let Some(enc) = rec.encoder.take() {
            let _ = enc.finish();
        }
    }

    for hwnd in a.frames.drain(..) {
        let _ = DestroyWindow(hwnd);
    }
    if a.chip != HWND::default() {
        let _ = DestroyWindow(a.chip);
        a.chip = HWND::default();
    }

    let out = a.output.take().unwrap_or_default();
    let name = std::path::Path::new(&out)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(out);
    show_toast(&format!("已保存到桌面：{name}"));
}

unsafe extern "system" fn chip_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let (w, h) = (s(208), s(38));
            let full = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            fill(hdc, &full, C_BG);
            let a = app();
            let secs = a.started_at.elapsed().as_secs();
            let mut r = RECT {
                left: s(10),
                top: 0,
                right: s(96),
                bottom: h,
            };
            draw_label(hdc, &format!("● {}", util::mmss(secs)), &mut r, true, C_ACCENT);
            let mut r = RECT {
                left: s(96),
                top: 0,
                right: w - s(8),
                bottom: h,
            };
            draw_label(hdc, "F9 停止", &mut r, false, C_MUTED);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            stop_record();
            LRESULT(0)
        }
        WM_HOTKEY => {
            stop_record();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            fill(hdc, &r, C_ACCENT);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- toast

unsafe fn show_toast(text: &str) {
    let a = app();
    let w = s(460);
    let h = s(64);
    let x = a.mon.left + (a.mon.right - a.mon.left - w) / 2;
    let y = a.mon.bottom - s(160);
    if let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        CLS_TOAST,
        w!(""),
        WS_POPUP,
        x,
        y,
        w,
        h,
        None,
        None,
        Some(a.hinst),
        None,
    ) {
        a.toast = hwnd;
        let ptr = Box::into_raw(Box::new(util::wide(text)));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
        round_window(hwnd, w, h, s(14));
        let _ = ShowWindow(hwnd, SW_SHOW);
        SetTimer(Some(hwnd), 2, 2600, None);
    }
}

unsafe extern "system" fn toast_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let (w, h) = (s(460), s(64));
            let full = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            fill(hdc, &full, C_BG);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Vec<u16>;
            if !ptr.is_null() {
                let mut buf = (*ptr).clone();
                let a = app();
                let old = SelectObject(hdc, HGDIOBJ(a.hfont.0));
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, C_TEXT);
                let mut r = RECT {
                    left: s(16),
                    top: 0,
                    right: w - s(16),
                    bottom: h,
                };
                DrawTextW(hdc, &mut buf, &mut r, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
