//! 系统声音采集：WASAPI loopback（渲染端点的环回）。
//!
//! 不混音、不碰麦克风——录的就是扬声器里正在放的东西。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use windows::core::GUID;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use crate::rec::Recorder;

/// MMDeviceEnumerator 的 CLSID
const CLSID_MMDEVICE_ENUMERATOR: GUID = GUID::from_u128(0xBCDE0395_E52F_467C_8E3D_C4579291692E);

#[derive(Clone, Copy, Debug)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
}

struct Endpoint {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    info: AudioInfo,
    block_align: u32,
    is_float: bool,
}

unsafe fn open_endpoint() -> Result<Endpoint, String> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&CLSID_MMDEVICE_ENUMERATOR, None, CLSCTX_ALL)
            .map_err(|e| format!("CoCreateInstance: {e}"))?;
    let device = enumerator
        .GetDefaultAudioEndpoint(eRender, eConsole)
        .map_err(|e| format!("GetDefaultAudioEndpoint: {e}"))?;
    let client: IAudioClient = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("Activate: {e}"))?;
    let fmt_ptr = client.GetMixFormat().map_err(|e| format!("GetMixFormat: {e}"))?;
    let fmt = &*fmt_ptr;
    let info = AudioInfo {
        sample_rate: fmt.nSamplesPerSec,
        channels: fmt.nChannels,
    };
    let block_align = fmt.nBlockAlign as u32;
    let is_float = fmt.wBitsPerSample == 32;

    client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            10_000_000, // 1s 缓冲
            0,
            fmt_ptr,
            None,
        )
        .map_err(|e| format!("Initialize(loopback): {e}"))?;

    let capture: IAudioCaptureClient = client.GetService().map_err(|e| format!("GetService: {e}"))?;
    client.Start().map_err(|e| format!("Start: {e}"))?;

    Ok(Endpoint {
        client,
        capture,
        info,
        block_align,
        is_float,
    })
}

/// 探测默认播放设备；没有设备就返回 None（只录画面）。
pub fn probe() -> Option<AudioInfo> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut ep = match open_endpoint() {
            Ok(ep) => ep,
            Err(e) => {
                crate::log(&format!("audio probe failed: {e}"));
                return None;
            }
        };
        let info = ep.info;
        crate::log(&format!("audio probe ok: {}Hz {}ch", info.sample_rate, info.channels));
        let _ = ep.client.Stop();
        Some(info)
    }
}

pub fn spawn(
    info: AudioInfo,
    cb: Arc<Mutex<Recorder>>,
    started: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut ep = match open_endpoint() {
            Ok(ep) => ep,
            Err(e) => {
                crate::log(&format!("audio open failed: {e}"));
                return;
            }
        };
        crate::log(&format!(
            "audio open ok: {}Hz {}ch float={} block_align={}",
            ep.info.sample_rate, ep.info.channels, ep.is_float, ep.block_align
        ));
        // 设备可能在探测和真正开录之间换了，以实际端点为准
        let _ = info;

        let mut pcm: Vec<u8> = Vec::with_capacity(8192);
        let mut pushed = 0u64;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let mut packet = ep.capture.GetNextPacketSize().unwrap_or(0);
            while packet > 0 {
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if ep
                    .capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }
                if frames > 0 {
                    let bytes = frames * ep.block_align;
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        pcm.clear();
                        pcm.resize(bytes as usize / if ep.is_float { 2 } else { 1 }, 0);
                    } else {
                        pcm.clear();
                        let src = std::slice::from_raw_parts(data, bytes as usize);
                        if ep.is_float {
                            pcm.reserve(src.len() / 2);
                            for chunk in src.chunks_exact(4) {
                                let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                                let v = (f.clamp(-1.0, 1.0) * 32767.0) as i16;
                                pcm.extend_from_slice(&v.to_le_bytes());
                            }
                        } else {
                            pcm.extend_from_slice(src);
                        }
                    }
                    if started.load(Ordering::Relaxed) && !pcm.is_empty() {
                        cb.lock().push_audio(&pcm);
                        pushed += 1;
                        if pushed == 1 {
                            crate::log(&format!("audio first packet: {} bytes", pcm.len()));
                        }
                    }
                }
                let _ = ep.capture.ReleaseBuffer(frames);
                packet = ep.capture.GetNextPacketSize().unwrap_or(0);
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let _ = ep.client.Stop();
    })
}
