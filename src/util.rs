use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

pub fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn desktop_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let desktop = home.join("Desktop");
    if desktop.is_dir() {
        desktop
    } else {
        home
    }
}

pub fn now_stamp() -> String {
    unsafe {
        let st = windows::Win32::System::SystemInformation::GetLocalTime();
        format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

pub fn mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
