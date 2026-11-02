//! 麦克风采集：cpal 在自己的线程里建流、保活，采到的 PCM 直接喂给编码器。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

use crate::rec::Recorder;

#[derive(Clone, Copy, Debug)]
pub struct MicInfo {
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn probe() -> Option<MicInfo> {
    let host = cpal::default_host();
    let device = host.default_input_device()?;
    let cfg = device.default_input_config().ok()?;
    Some(MicInfo {
        sample_rate: cfg.sample_rate().0,
        channels: cfg.channels(),
    })
}

pub fn spawn(
    info: MicInfo,
    cb: Arc<Mutex<Recorder>>,
    started: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            return;
        };
        let config = cpal::StreamConfig {
            channels: info.channels,
            sample_rate: cpal::SampleRate(info.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let push = move |pcm: Vec<u8>| {
            if pcm.is_empty() {
                return;
            }
            cb.lock().push_audio(&pcm);
        };

        let stream = match device.default_input_config() {
            Ok(cfg) if cfg.sample_format() == cpal::SampleFormat::I16 => {
                let started = started.clone();
                let push = push.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        if !started.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut pcm = Vec::with_capacity(data.len() * 2);
                        for s in data {
                            pcm.extend_from_slice(&s.to_le_bytes());
                        }
                        push(pcm);
                    },
                    |e| eprintln!("[audio] {e}"),
                    None,
                )
            }
            _ => {
                let started = started.clone();
                let push = push.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        if !started.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut pcm = Vec::with_capacity(data.len() * 2);
                        for s in data {
                            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                            pcm.extend_from_slice(&v.to_le_bytes());
                        }
                        push(pcm);
                    },
                    |e| eprintln!("[audio] {e}"),
                    None,
                )
            }
        };

        let Ok(stream) = stream else {
            return;
        };
        if stream.play().is_err() {
            return;
        }
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(40));
        }
        drop(stream);
    })
}
