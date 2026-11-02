#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod rec;
mod util;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SetFocus, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, ShellExecuteW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLS_OVERLAY: PCWSTR = w!("SaicordOverlay");
const CLS_BAR: PCWSTR = w!("SaicordBar");
const CLS_CHIP: PCWSTR = w!("SaicordChip");
const CLS_FRAME: PCWSTR = w!("SaicordFrame");
const CLS_TOAST: PCWSTR = w!("SaicordToast");
const CLS_TRAY: PCWSTR = w!("SaicordTray");

const WM_APP_START: u32 = WM_APP + 1;
const WM_APP_TRAY: u32 = WM_APP + 2;

static TRAY_ONLY: AtomicBool = AtomicBool::new(false);

const HOTKEY_START: i32 = 1;
const HOTKEY_STOP: i32 = 2;

const MENU_START: usize = 1;
const MENU_STOP: usize = 2;
const MENU_CONFIG: usize = 3;
const MENU_QUIT: usize = 4;
const MENU_FPS: usize = 5;

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
    bmp: HBITMAP,
    old: HGDIOBJ,
    ptr: *mut u32,
    w: i32,
    h: i32,
}

struct App {
    hinst: HINSTANCE,
    tray: HWND,
    hicon: HICON,
    cfg: config::Config,
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
    selecting: bool,
    started_at: Instant,
    output: Option<String>,
    ctl: Option<rec::Control>,
    audio_stop: Arc<AtomicBool>,
    audio_thread: Option<std::thread::JoinHandle<()>>,
    first_frame: Arc<AtomicBool>,
    frame_count: Arc<AtomicU64>,
    bar_hover: i32,
    timer_pending: bool,
}

static mut APP: Option<App> = None;

#[allow(static_mut_refs)]
fn app() -> &'static mut App {
    unsafe { APP.as_mut().expect("app not initialised") }
}

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
    let path = match std::env::var("SAICORD_LOG") {
        Ok(p) if !p.is_empty() => std::path::PathBuf::from(p),
        _ => match std::env::var("LOCALAPPDATA") {
            Ok(dir) => {
                let dir = std::path::PathBuf::from(dir).join("saicord");
                let _ = std::fs::create_dir_all(&dir);
                dir.join("saicord.log")
            }
            Err(_) => return,
        },
    };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}


// ---------------------------------------------------------------- 测试模式
// 无窗口、无遮罩：`saicord.exe --test <mode> ...`，只写日志不弹任何界面。
//   encode  <w> <h> <fps> <secs> <out>            合成画面走同一个编码器
//   capture <x> <y> <w> <h> <secs> <out> [audio]  真实采集指定矩形（物理像素）
//   info                                          显示器物理尺寸
fn test_mode(args: &[String]) {
    use windows_capture::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    };

    let mode = args.first().map(|s| s.as_str()).unwrap_or("");
    match mode {
        "info" => unsafe {
            let w = GetSystemMetrics(SM_CXSCREEN);
            let h = GetSystemMetrics(SM_CYSCREEN);
            log(&format!("info: screen {}x{}", w, h));
        },
        "encode" => {
            let w: u32 = args[1].parse().unwrap();
            let h: u32 = args[2].parse().unwrap();
            let fps: u32 = args[3].parse().unwrap();
            let secs: u32 = args[4].parse().unwrap();
            let out = args[5].clone();
            let n = (fps as u64) * (secs as u64);

            let bitrate = if w * h > 1920 * 1080 { 16_000_000 } else { 8_000_000 };
            let video = VideoSettingsBuilder::new(w, h)
                .sub_type(VideoSettingsSubType::H264)
                .frame_rate(fps)
                .bitrate(bitrate);
            let audio = AudioSettingsBuilder::new().disabled(true);
            let mut enc = match VideoEncoder::new(
                video,
                audio,
                ContainerSettingsBuilder::default(),
                &out,
            ) {
                Ok(e) => e,
                Err(e) => {
                    log(&format!("encode: open failed {e}"));
                    return;
                }
            };

            // 静态底：渐变 + 棋盘，保证有细节可测画质
            let mut bg = vec![0u8; (w * h * 4) as usize];
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let checker = ((x / 32 + y / 32) % 2) as u8;
                    bg[i] = (x * 255 / w.max(1)) as u8;
                    bg[i + 1] = (y * 255 / h.max(1)) as u8;
                    bg[i + 2] = checker * 180;
                    bg[i + 3] = 255;
                }
            }
            let mut buf = bg.clone();
            let t0 = Instant::now();
            let mut worst = Duration::ZERO;
            for i in 0..n {
                buf.copy_from_slice(&bg);
                let bx = ((i * 7) % w.max(1) as u64) as u32;
                let by = ((i * 3) % h.max(1) as u64) as u32;
                for y in by..(by + 96).min(h) {
                    for x in bx..(bx + 96).min(w) {
                        let p = ((y * w + x) * 4) as usize;
                        buf[p..p + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
                // 帧号条码：16 位，每位 16x16，画在缓冲顶部（落到成片底部）
                for bit in 0..16 {
                    let on = (i >> bit) & 1 == 1;
                    let v = if on { 255u8 } else { 0u8 };
                    for y in 0..16u32 {
                        for x in 0..16u32 {
                            let px = bit as u32 * 16 + x;
                            let p = ((y * w + px) * 4) as usize;
                            buf[p] = v;
                            buf[p + 1] = v;
                            buf[p + 2] = v;
                            buf[p + 3] = 255;
                        }
                    }
                }
                let ts = (i as i64) * 10_000_000 / fps as i64;
                let s = Instant::now();
                if let Err(e) = enc.send_frame_buffer(&buf, ts) {
                    log(&format!("encode: frame {i} failed {e}"));
                    break;
                }
                let d = s.elapsed();
                if d > worst {
                    worst = d;
                }
                let target = Duration::from_nanos((i + 1) * 1_000_000_000 / fps as u64);
                let el = t0.elapsed();
                if target > el {
                    std::thread::sleep(target - el);
                }
            }
            let _ = enc.finish();
            log(&format!(
                "encode done: {}x{}@{} frames={} wall={:.2}s worst_send={:.1}ms",
                w,
                h,
                fps,
                n,
                t0.elapsed().as_secs_f64(),
                worst.as_secs_f64() * 1000.0
            ));
        }
        "capture" => {
            let x: i32 = args[1].parse().unwrap();
            let y: i32 = args[2].parse().unwrap();
            let w: i32 = args[3].parse().unwrap();
            let h: i32 = args[4].parse().unwrap();
            let secs: u32 = args[5].parse().unwrap();
            let out = args[6].clone();
            let want_audio = args.get(7).map(|s| s == "audio").unwrap_or(false);

            let started = Arc::new(AtomicBool::new(false));
            let frame_count = Arc::new(AtomicU64::new(0));
            let dev = if want_audio { audio::probe() } else { None };
            let audio_cfg = dev.map(|m| (m.sample_rate, m.channels));
            log(&format!(
                "capture: rect {x},{y} {w}x{h} secs={secs} audio={:?}",
                audio_cfg
            ));
            let t_start = Instant::now();
            let fps: u32 = args.get(8).and_then(|v| v.parse().ok()).unwrap_or(60);
            let mut ctl = match rec::start(
                1,
                (x.max(0) as u32, y.max(0) as u32, (x + w).max(0) as u32, (y + h).max(0) as u32),
                out.clone(),
                audio_cfg,
                fps,
                started.clone(),
                frame_count.clone(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    log(&format!("capture: start failed {e}"));
                    return;
                }
            };
            log(&format!("capture: start took {:.0}ms", t_start.elapsed().as_secs_f64() * 1000.0));
            let cb = ctl.callback();
            let stop = Arc::new(AtomicBool::new(false));
            let audio_thread = dev.map(|info| audio::spawn(info, cb.clone(), started.clone(), stop.clone()));

            let t0 = Instant::now();
            std::thread::sleep(Duration::from_secs(secs as u64));
            stop.store(true, Ordering::Relaxed);
            if let Some(h) = audio_thread {
                let _ = h.join();
            }
            let cb2 = ctl.callback();
            let _ = ctl.stop();
            let mut r = cb2.lock();
            if let Some(e) = r.encoder.take() {
                let _ = e.finish();
            }
            drop(r);
            let n = frame_count.load(Ordering::Relaxed);
            if n == 0 {
                let _ = std::fs::remove_file(&out);
            }
            log(&format!(
                "capture done: frames={} wall={:.2}s out={out}",
                frame_count.load(Ordering::Relaxed),
                t0.elapsed().as_secs_f64()
            ));
        }
        "loop" => {
            // 反复起停，看句柄/内存有没有泄漏
            let n: u32 = args[1].parse().unwrap();
            let secs: u32 = args[2].parse().unwrap();
            let dir = args[3].clone();
            for i in 0..n {
                let out = format!("{dir}/loop_{i}.mp4");
                let started = Arc::new(AtomicBool::new(false));
                let frame_count = Arc::new(AtomicU64::new(0));
                let mut ctl = match rec::start(
                    1,
                    (3000, 1200, 3640, 1560),
                    out,
                    None,
                    60,
                    started.clone(),
                    frame_count.clone(),
                ) {
                    Ok(c) => c,
                    Err(e) => {
                        log(&format!("loop {i}: start failed {e}"));
                        continue;
                    }
                };
                std::thread::sleep(Duration::from_secs(secs as u64));
                let cb = ctl.callback();
                let _ = ctl.stop();
                let mut r = cb.lock();
                if let Some(e) = r.encoder.take() {
                    let _ = e.finish();
                }
                drop(r);
                unsafe {
                    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
                    let mut handles = 0u32;
                    let _ = GetProcessHandleCount(GetCurrentProcess(), &mut handles);
                    let cur = GetCurrentProcess();
                    let mut pmc = windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS::default();
                    let _ = windows::Win32::System::ProcessStatus::GetProcessMemoryInfo(
                        cur,
                        &mut pmc,
                        std::mem::size_of_val(&pmc) as u32,
                    );
                    log(&format!(
                        "loop {i}: frames={} handles={} ws={}MB",
                        frame_count.load(Ordering::Relaxed),
                        handles,
                        pmc.WorkingSetSize / 1024 / 1024
                    ));
                }
            }
        }
        other => log(&format!("test: unknown mode {other}")),
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(|s| s.as_str()) == Some("--test") {
        test_mode(&argv[1..]);
        return;
    }
    // --tray：只驻留托盘，不弹框选（开机自启用）
    TRAY_ONLY.store(argv.iter().any(|a| a == "--tray"), Ordering::Relaxed);
    let hmodule = unsafe { GetModuleHandleW(None) }.expect("module handle");
    let hinst = HINSTANCE(hmodule.0);

    // 单实例：再次启动只是叫醒已经在托盘里的那个
    unsafe {
        let mtx = CreateMutexW(None, true, w!("Global\\saicord-single-instance"));
        let lasterr = GetLastError();
        log(&format!("main start pid={} mutex_ok={} lasterr={:?}", std::process::id(), mtx.is_ok(), lasterr));
        if lasterr == ERROR_ALREADY_EXISTS {
            match FindWindowW(CLS_TRAY, PCWSTR::null()) {
                Ok(h) => {
                    log(&format!("second instance: wake {:?} -> {:?}", h.0, PostMessageW(Some(h), WM_APP_START, WPARAM(0), LPARAM(0))));
                }
                Err(e) => log(&format!("second instance: tray window not found: {e}")),
            }
            return;
        }
    }

    unsafe {
        APP = Some(App {
            hinst,
            tray: HWND::default(),
            hicon: HICON::default(),
            cfg: config::load(),
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
            selecting: false,
            started_at: Instant::now(),
            output: None,
            ctl: None,
            audio_stop: Arc::new(AtomicBool::new(false)),
            audio_thread: None,
            first_frame: Arc::new(AtomicBool::new(false)),
            frame_count: Arc::new(AtomicU64::new(0)),
            bar_hover: 0,
            timer_pending: false,
        });
        register_classes(app().hinst);
        create_tray();
        register_hotkeys();
        if !TRAY_ONLY.load(Ordering::Relaxed) {
            start_selection();
        }
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
    let classes: [(PCWSTR, WNDPROC); 6] = [
        (CLS_OVERLAY, Some(overlay_proc)),
        (CLS_BAR, Some(bar_proc)),
        (CLS_CHIP, Some(chip_proc)),
        (CLS_FRAME, Some(frame_proc)),
        (CLS_TOAST, Some(toast_proc)),
        (CLS_TRAY, Some(tray_proc)),
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

// ---------------------------------------------------------------- tray

unsafe fn make_icon() -> HICON {
    const SIZE: i32 = 32;
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE,
            biHeight: -SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let Ok(color) = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut ptr, None, 0) else {
        return LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
    };
    let px = std::slice::from_raw_parts_mut(ptr as *mut u32, (SIZE * SIZE) as usize);
    let c = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let v = if d <= 6.0 {
                0xFFFF_FFFF
            } else if d <= 14.0 {
                ACCENT
            } else {
                0
            };
            px[(y * SIZE + x) as usize] = v;
        }
    }
    let mask_bits = [0u8; 4 * 32];
    let mask = CreateBitmap(SIZE, SIZE, 1, 1, Some(mask_bits.as_ptr() as *const _));
    let info = ICONINFO {
        fIcon: TRUE,
        hbmMask: mask,
        hbmColor: color,
        ..Default::default()
    };
    CreateIconIndirect(&info).unwrap_or_else(|_| {
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    })
}

unsafe fn create_tray() {
    let a = app();
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        CLS_TRAY,
        w!("saicord"),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(a.hinst),
        None,
    ) else {
        return;
    };
    a.tray = hwnd;
    a.hicon = make_icon();

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_APP_TRAY,
        hIcon: a.hicon,
        ..Default::default()
    };
    let tip: Vec<u16> = format!("saicord · {} 框选 / {} 停止", a.cfg.start_hotkey, a.cfg.stop_hotkey)
        .encode_utf16()
        .collect();
    let n = tip.len().min(nid.szTip.len() - 1);
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
        log("Shell_NotifyIcon(NIM_ADD) failed");
    }
}

unsafe fn register_hotkeys() {
    let a = app();
    if let Some((mods, vk)) = config::parse_hotkey(&a.cfg.start_hotkey) {
        if RegisterHotKey(Some(a.tray), HOTKEY_START, HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT.0), vk).is_err() {
            log(&format!("start hotkey 注册失败: {}", a.cfg.start_hotkey));
            show_toast(&format!("快捷键 {} 被占用，开始键没生效", a.cfg.start_hotkey));
        }
    } else {
        log(&format!("start hotkey 解析失败: {}", a.cfg.start_hotkey));
        show_toast(&format!("开始键写法不对：{}", a.cfg.start_hotkey));
    }
    if let Some((mods, vk)) = config::parse_hotkey(&a.cfg.stop_hotkey) {
        if RegisterHotKey(Some(a.tray), HOTKEY_STOP, HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT.0), vk).is_err() {
            log(&format!("stop hotkey 注册失败: {}", a.cfg.stop_hotkey));
            show_toast(&format!("快捷键 {} 被占用，停止键没生效", a.cfg.stop_hotkey));
        }
    } else {
        log(&format!("stop hotkey 解析失败: {}", a.cfg.stop_hotkey));
        show_toast(&format!("停止键写法不对：{}", a.cfg.stop_hotkey));
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let a = app();
    let Ok(menu) = CreatePopupMenu() else { return };
    let recording = a.recording;
    let start_label = util::wide(&format!("开始录制\t{}", a.cfg.start_hotkey));
    let stop_label = util::wide(&format!("停止录制\t{}", a.cfg.stop_hotkey));
    let cfg_label = util::wide("打开配置文件");
    let quit_label = util::wide("退出");
    let tier = match a.cfg.fps {
        0..=45 => "低",
        46..=90 => "中",
        _ => "高",
    };
    let fps_label = util::wide(&format!("帧率：{} {} fps（点击切换）", tier, a.cfg.fps));

    if recording {
        let _ = AppendMenuW(menu, MF_STRING, MENU_STOP, PCWSTR(stop_label.as_ptr()));
    } else {
        let _ = AppendMenuW(menu, MF_STRING, MENU_START, PCWSTR(start_label.as_ptr()));
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    if !recording {
        let _ = AppendMenuW(menu, MF_STRING, MENU_FPS, PCWSTR(fps_label.as_ptr()));
    }
    let _ = AppendMenuW(menu, MF_STRING, MENU_CONFIG, PCWSTR(cfg_label.as_ptr()));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(quit_label.as_ptr()));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, None, hwnd, None).0;
    let _ = DestroyMenu(menu);

    match cmd as usize {
        MENU_START => start_selection(),
        MENU_STOP => stop_record(),
        MENU_FPS => {
            let a = app();
            a.cfg.fps = match a.cfg.fps {
                ..=45 => 60,
                46..=90 => 120,
                _ => 30,
            };
            config::save(&a.cfg);
            let tier = match a.cfg.fps {
                ..=45 => "低",
                46..=90 => "中",
                _ => "高",
            };
            show_toast(&format!("帧率：{} {} fps（下次录制生效）", tier, a.cfg.fps));
        }
        MENU_CONFIG => open_config(),
        MENU_QUIT => quit_app(),
        _ => {}
    }
}

unsafe fn open_config() {
    let p = config::path();
    if !p.exists() {
        config::save(&app().cfg);
    }
    let path = util::wide(&p.to_string_lossy());
    let verb = util::wide("open");
    ShellExecuteW(
        None,
        PCWSTR(verb.as_ptr()),
        PCWSTR(path.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
}

unsafe fn quit_app() {
    let a = app();
    if a.recording {
        stop_record();
    }
    if a.tray != HWND::default() {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: a.tray,
            uID: 1,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = DestroyWindow(a.tray);
        a.tray = HWND::default();
    }
    PostQuitMessage(0);
}

unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_APP_START => {
            start_selection();
            LRESULT(0)
        }
        WM_APP_TRAY => {
            match lp.0 as u32 {
                WM_LBUTTONUP => start_selection(),
                WM_RBUTTONUP => show_tray_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            match wp.0 as i32 {
                HOTKEY_START => start_selection(),
                HOTKEY_STOP => stop_record(),
                _ => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- selection

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

unsafe fn start_selection() {
    let a = app();
    if a.recording || a.selecting {
            return;
    }
    open_overlay();
}

unsafe fn open_overlay() {
    let (mon, name) = monitor_under_cursor();
    let idx = monitor_index_for(&name);
    let w = mon.right - mon.left;
    let h = mon.bottom - mon.top;

    let a = app();
    if a.overlay != HWND::default() {
        let _ = DestroyWindow(a.overlay);
        a.overlay = HWND::default();
    }
    a.mon = mon;
    a.mon_index = idx;
    a.sel = None;
    a.drag = None;
    a.selecting = true;

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
    let old = SelectObject(mem, HGDIOBJ(bmp.0));
    ReleaseDC(None, screen);

    a.dib = Some(Dib {
        hdc: mem,
        bmp,
        old,
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

    let dpi = GetDpiForWindow(hwnd);
    a.scale = if dpi >= 96 { dpi as f32 / 96.0 } else { 1.0 };
    make_fonts();

    paint_overlay();
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(hwnd));
}

unsafe fn cancel_selection() {
    let a = app();
    a.selecting = false;
    a.sel = None;
    a.drag = None;
    if a.overlay != HWND::default() {
        let _ = DestroyWindow(a.overlay);
        a.overlay = HWND::default();
    }
    if a.bar != HWND::default() {
        let _ = DestroyWindow(a.bar);
        a.bar = HWND::default();
    }
    destroy_frames();
    free_dib();
}

/// 选完就把遮罩位图还掉：4K 屏上它一个人就占 31.6 MB。
unsafe fn free_dib() {
    let a = app();
    if let Some(dib) = a.dib.take() {
        let _ = SelectObject(dib.hdc, dib.old);
        let _ = DeleteObject(HGDIOBJ(dib.bmp.0));
        let _ = DeleteDC(dib.hdc);
    }
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
                let sel = a.sel.unwrap();
                a.selecting = false;
                if a.overlay != HWND::default() {
                    let _ = DestroyWindow(a.overlay);
                    a.overlay = HWND::default();
                }
                free_dib();
                destroy_frames();
                create_frame(sel);
                show_bar();
            } else {
                cancel_selection();
            }
            LRESULT(0)
        }
        WM_RBUTTONUP | WM_MBUTTONUP => {
            cancel_selection();
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == 0x1B {
                cancel_selection();
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
            let mut r = RECT {
                left: s(8),
                top: pad,
                right: s(96),
                bottom: bh - pad,
            };
            if a.bar_hover == 1 {
                fill(hdc, &r, C_HOVER);
            }
            draw_label(hdc, "● 开始", &mut r, true, C_ACCENT);

            let mut r = RECT {
                left: s(100),
                top: pad,
                right: s(196),
                bottom: bh - pad,
            };
            if a.bar_hover == 2 {
                fill(hdc, &r, C_HOVER);
            }
            let audio_text = if a.audio_dev.is_none() {
                "无音频设备"
            } else if a.audio_on {
                "系统声 开"
            } else {
                "系统声 关"
            };
            draw_label(hdc, audio_text, &mut r, false, C_MUTED);

            let mut r = RECT {
                left: s(200),
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
                cancel_selection();
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == 0x1B {
                cancel_selection();
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
    // 硬件编码器（NVENC）对最小分辨率有限制：约 145×49，留点余量
    if sel.right - sel.left < 160 || sel.bottom - sel.top < 64 {
        show_toast("选区太小：至少 160×64 像素");
        return;
    }
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

    // 启动时设备可能还没就绪，开录前再探一次
    if a.audio_dev.is_none() && a.audio_on {
        a.audio_dev = audio::probe();
    }
    let audio_cfg = if a.audio_on {
        a.audio_dev.map(|m| (m.sample_rate, m.channels))
    } else {
        None
    };
    a.first_frame = Arc::new(AtomicBool::new(false));
    a.frame_count = Arc::new(AtomicU64::new(0));

    // 编码器初始化要 ~1s：先给用户看到录制框和计时条，再去启动采集
    a.started_at = Instant::now();
    a.timer_pending = true;
    a.recording = true;
    if a.frames.is_empty() {
        create_frame(sel);
    }
    create_chip(sel);

    match rec::start(
        a.mon_index,
        crop,
        out.clone(),
        audio_cfg,
        a.cfg.fps,
        a.first_frame.clone(),
        a.frame_count.clone(),
    ) {
        Ok(ctl) => {
            let cb = ctl.callback();
            a.ctl = Some(ctl);
            a.output = Some(out);
            a.audio_stop = Arc::new(AtomicBool::new(false));

            if let Some(info) = a.audio_dev.filter(|_| a.audio_on) {
                let handle = audio::spawn(info, cb, a.first_frame.clone(), a.audio_stop.clone());
                a.audio_thread = Some(handle);
            }

        }
        Err(e) => {
            log(&format!("rec start failed: {e}"));
            a.recording = false;
            // 编码器可能已经建了半个文件，删掉别留垃圾
            let _ = std::fs::remove_file(&out);
            destroy_frames();
            if a.chip != HWND::default() {
                let _ = DestroyWindow(a.chip);
                a.chip = HWND::default();
            }
            show_toast(&format!("启动录制失败：{e}"));
        }
    }
}

unsafe fn destroy_frames() {
    let a = app();
    for hwnd in a.frames.drain(..) {
        let _ = DestroyWindow(hwnd);
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

    a.audio_stop.store(true, Ordering::Relaxed);
    if let Some(handle) = a.audio_thread.take() {
        let _ = handle.join();
    }

    let mut save_err = None;
    if let Some(ctl) = a.ctl.take() {
        let cb = ctl.callback();
        let _ = ctl.stop();
        let mut rec = cb.lock();
        if let Some(enc) = rec.encoder.take() {
            if let Err(e) = enc.finish() {
                log(&format!("finish failed: {e}"));
                save_err = Some(e.to_string());
            }
        }
    }

    destroy_frames();
    if a.chip != HWND::default() {
        let _ = DestroyWindow(a.chip);
        a.chip = HWND::default();
    }

    let secs = a.started_at.elapsed().as_secs_f64();
    log(&format!(
        "record stop: frames={} secs={:.2} expected≈{}",
        a.frame_count.load(Ordering::Relaxed),
        secs,
        (secs * a.cfg.fps as f64).round() as u64
    ));

    let out = a.output.take().unwrap_or_default();
    let name = std::path::Path::new(&out)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| out.clone());
    let frames = a.frame_count.load(Ordering::Relaxed);
    match save_err {
        Some(e) => {
            let _ = std::fs::remove_file(&out);
            show_toast(&format!("保存失败：{e}"));
        }
        None if frames == 0 => {
            // 一帧都没采到：MF 仍然会写出一个 0 字节的坏文件
            let _ = std::fs::remove_file(&out);
            show_toast("没录到画面，已取消");
        }
        None => show_toast(&format!("已保存到桌面：{name}")),
    }
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
            // 编码器要 ~1s 才出第一帧：这之前显示「启动中」，计时从第一帧开始
            let ready = a.first_frame.load(Ordering::Relaxed);
            if ready && a.timer_pending {
                a.started_at = Instant::now();
                a.timer_pending = false;
            }
            let label = if ready {
                format!("● {}", util::mmss(a.started_at.elapsed().as_secs()))
            } else {
                "● 启动中".to_string()
            };
            let mut r = RECT {
                left: s(10),
                top: 0,
                right: s(96),
                bottom: h,
            };
            draw_label(hdc, &label, &mut r, true, C_ACCENT);
            let mut r = RECT {
                left: s(96),
                top: 0,
                right: w - s(8),
                bottom: h,
            };
            let hint = format!("{} 停止", a.cfg.stop_hotkey);
            draw_label(hdc, &hint, &mut r, false, C_MUTED);
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
                DrawTextW(hdc, &mut buf, &mut r, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
