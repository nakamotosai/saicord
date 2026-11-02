//! 配置：`%APPDATA%\saicord\config.txt`，纯文本，改完重启生效。

use std::path::PathBuf;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub start_hotkey: String,
    pub stop_hotkey: String,
    /// 录制帧率：24 / 30 / 60 / 90 / 120
    pub fps: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            start_hotkey: "F8".into(),
            stop_hotkey: "F9".into(),
            fps: 60,
        }
    }
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("saicord").join("config.txt")
}

pub fn load() -> Config {
    let p = path();
    let Ok(text) = std::fs::read_to_string(&p) else {
        let cfg = Config::default();
        save(&cfg);
        return cfg;
    };
    let mut cfg = Config::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
        if v.is_empty() {
            continue;
        }
        match k.as_str() {
            "start_hotkey" => cfg.start_hotkey = v,
            "stop_hotkey" => cfg.stop_hotkey = v,
            "fps" => {
                if let Ok(n) = v.parse::<u32>() {
                    cfg.fps = n.clamp(15, 144);
                }
            }
            _ => {}
        }
    }
    cfg
}

pub fn save(cfg: &Config) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body = format!(
        "# saicord 配置（改完重启生效）\n\
         # 键名写法：Ctrl / Alt / Shift / Win 用 + 连接，例如 F8、F9、Shift+F8\n\
         # 帧率：24 / 30 / 60 / 90 / 120，越高越丝滑、文件越大\n\
         fps          = {}\n\
         start_hotkey = {}\n\
         stop_hotkey  = {}\n",
        cfg.fps, cfg.start_hotkey, cfg.stop_hotkey
    );
    let _ = std::fs::write(p, body);
}

/// "Ctrl+Alt+R" → (MOD_CONTROL|MOD_ALT, 'R')
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut vk = 0u32;
    for raw in s.split('+') {
        let part = raw.trim().to_ascii_uppercase();
        if part.is_empty() {
            continue;
        }
        match part.as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CONTROL.0,
            "ALT" => mods |= MOD_ALT.0,
            "SHIFT" => mods |= MOD_SHIFT.0,
            "WIN" | "META" | "SUPER" => mods |= MOD_WIN.0,
            other => vk = vk_for(other)?,
        }
    }
    if vk == 0 {
        None
    } else {
        Some((mods, vk))
    }
}

fn vk_for(name: &str) -> Option<u32> {
    if name.len() == 1 {
        let c = name.chars().next()?;
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
    }
    if let Some(num) = name.strip_prefix('F') {
        if let Ok(n) = num.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1);
            }
        }
    }
    let vk = match name {
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "ESC" | "ESCAPE" => 0x1B,
        "ENTER" | "RETURN" => 0x0D,
        "BACKSPACE" => 0x08,
        "INSERT" => 0x2D,
        "DELETE" | "DEL" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "PRINTSCREEN" | "PRTSC" => 0x2C,
        "PAUSE" => 0x13,
        "CAPSLOCK" => 0x14,
        _ => return None,
    };
    Some(vk)
}
