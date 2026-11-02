//! 自动更新:查 GitHub 最新 release → 托盘提示 → 下载 → 替换自身并重启。
//!
//! HTTP 走系统自带的 WinHTTP(schannel),不引第三方网络库,单文件体积不变。

use std::path::{Path, PathBuf};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

const API_HOST: &str = "api.github.com";
const API_PATH: &str = "/repos/nakamotosai/saicord/releases/latest";

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub url: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 一次 HTTPS GET,返回响应体;失败给出可读原因。
unsafe fn http_get(host: &str, path: &str) -> Result<Vec<u8>, String> {
    /// 句柄守卫:出错也不会漏关
    struct Closer(*mut core::ffi::c_void);
    impl Drop for Closer {
        fn drop(&mut self) {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }

    let agent = HSTRING::from(format!("saicord/{}", CURRENT));
    let session = WinHttpOpen(
        PCWSTR(agent.as_ptr()),
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        PCWSTR::null(),
        PCWSTR::null(),
        0,
    );
    if session.is_null() {
        return Err("WinHttpOpen 失败".into());
    }
    let _session = Closer(session);

    let h = wide(host);
    let connect = WinHttpConnect(session, PCWSTR(h.as_ptr()), 443, 0);
    if connect.is_null() {
        return Err(format!("连不上 {host}"));
    }
    let _connect = Closer(connect);

    let p = wide(path);
    let request = WinHttpOpenRequest(
        connect,
        w!("GET"),
        PCWSTR(p.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        std::ptr::null(),
        WINHTTP_FLAG_SECURE,
    );
    if request.is_null() {
        return Err("WinHttpOpenRequest 失败".into());
    }
    let _request = Closer(request);

    // GitHub release 资产会 302 到 objects.githubusercontent.com
    let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
    let _ = WinHttpSetOption(
        Some(request as *const _),
        WINHTTP_OPTION_REDIRECT_POLICY,
        Some(&policy.to_ne_bytes()),
    );

    WinHttpSendRequest(request, None, None, 0, 0, 0).map_err(|e| format!("发送失败: {e}"))?;
    WinHttpReceiveResponse(request, std::ptr::null_mut()).map_err(|e| format!("收响应失败: {e}"))?;

    let mut status: u32 = 0;
    let mut len = std::mem::size_of::<u32>() as u32;
    let _ = WinHttpQueryHeaders(
        request,
        WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
        PCWSTR::null(),
        Some(&mut status as *mut u32 as *mut core::ffi::c_void),
        &mut len,
        std::ptr::null_mut(),
    );
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }

    let mut body = Vec::with_capacity(64 * 1024);
    loop {
        let mut avail: u32 = 0;
        WinHttpQueryDataAvailable(request, &mut avail).map_err(|e| format!("查询数据失败: {e}"))?;
        if avail == 0 {
            break;
        }
        let mut chunk = vec![0u8; avail as usize];
        let mut read: u32 = 0;
        WinHttpReadData(
            request,
            chunk.as_mut_ptr() as *mut core::ffi::c_void,
            avail,
            &mut read,
        )
        .map_err(|e| format!("读取失败: {e}"))?;
        if read == 0 {
            break;
        }
        chunk.truncate(read as usize);
        body.extend_from_slice(&chunk);
        if body.len() > 64 * 1024 * 1024 {
            break;
        }
    }
    Ok(body)
}

/// 从 JSON 文本里抠出第一个匹配键的值(不引 JSON 库)。
fn extract<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let i = text.find(key)? + key.len();
    let rest = &text[i..];
    let j = rest.find('"')?;
    Some(&rest[..j])
}

pub fn latest() -> Result<Release, String> {
    let body = unsafe { http_get(API_HOST, API_PATH)? };
    let text = String::from_utf8_lossy(&body);
    let tag = extract(&text, "\"tag_name\":\"").ok_or("响应里没有 tag_name")?.to_string();
    // 资源里挑 saicord.exe
    let url = text
        .match_indices("\"browser_download_url\":\"")
        .map(|(i, m)| {
            let rest = &text[i + m.len()..];
            &rest[..rest.find('"').unwrap_or(0)]
        })
        .find(|u| u.ends_with("/saicord.exe"))
        .ok_or("该 release 没有 saicord.exe 资源")?
        .to_string();
    Ok(Release { tag, url })
}

/// "v0.5.2" vs "0.5.1"：远端更新返回 true。
pub fn is_newer(remote: &str, current: &str) -> bool {
    fn parse(v: &str) -> Vec<u32> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|x| x.parse::<u32>().unwrap_or(0))
            .collect()
    }
    let (r, c) = (parse(remote), parse(current));
    for i in 0..r.len().max(c.len()) {
        let a = r.get(i).copied().unwrap_or(0);
        let b = c.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

pub fn update_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("saicord").join("update")
}

/// 下载到 update_dir/saicord-<tag>.exe
pub fn download(url: &str, tag: &str) -> Result<PathBuf, String> {
    let host = url
        .strip_prefix("https://")
        .ok_or("只支持 https")?
        .split('/')
        .next()
        .ok_or("URL 不合法")?;
    let path = &url[("https://".len() + host.len())..];
    let body = unsafe { http_get(host, path)? };
    if body.len() < 256 * 1024 {
        return Err(format!("下载内容只有 {} 字节,不像 exe", body.len()));
    }
    // PE 头校验:别把 HTML 错误页当 exe 装上
    if &body[..2] != b"MZ" {
        return Err("下载内容不是可执行文件".into());
    }
    let dir = update_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("建目录失败: {e}"))?;
    let out = dir.join(format!("saicord-{tag}.exe"));
    std::fs::write(&out, &body).map_err(|e| format!("写文件失败: {e}"))?;
    Ok(out)
}

/// 用新 exe 替换当前 exe:先把在跑的 exe 改名(Windows 允许),再把新的放上去。
pub fn apply(new_exe: &Path, current: &Path) -> Result<(), String> {
    let name = current
        .file_name()
        .ok_or("取不到自己的文件名")?
        .to_string_lossy()
        .to_string();
    let old = current.with_file_name(format!("{name}.old"));
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old).map_err(|e| format!("重命名当前 exe 失败: {e}"))?;
    if let Err(e) = std::fs::copy(new_exe, current) {
        let _ = std::fs::rename(&old, current);
        return Err(format!("写入新 exe 失败: {e}"));
    }
    Ok(())
}

/// 启动时清理上一轮升级留下的 .old
pub fn cleanup_old(current: &Path) {
    let name = current
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "saicord.exe".into());
    let old = current.with_file_name(format!("{name}.old"));
    if old.exists() {
        let _ = std::fs::remove_file(old);
    }
}
