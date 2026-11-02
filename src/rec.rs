//! 采集 + 编码 + 麦克风。
//!
//! 屏幕用 Windows Graphics Capture（GPU 侧裁切，只回读选区），
//! 编码走 Media Foundation 硬件 H.264，音频由 cpal 采麦克风后交给同一个编码器。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

pub type RecError = Box<dyn std::error::Error + Send + Sync>;
pub type Control = CaptureControl<Recorder, RecError>;

pub struct Flags {
    /// 相对显示器左上角的裁切矩形 (x0, y0, x1, y1)，物理像素
    pub crop: (u32, u32, u32, u32),
    pub out: String,
    /// Some((采样率, 声道数)) 表示要录麦克风
    pub audio: Option<(u32, u16)>,
    pub started: Arc<AtomicBool>,
}

pub struct Recorder {
    pub encoder: Option<VideoEncoder>,
    crop: (u32, u32, u32, u32),
    buf: Vec<u8>,
    started: Arc<AtomicBool>,
}

impl Recorder {
    pub fn push_audio(&mut self, pcm: &[u8]) {
        if let Some(enc) = self.encoder.as_mut() {
            let _ = enc.send_audio_buffer(pcm, 0);
        }
    }
}

impl GraphicsCaptureApiHandler for Recorder {
    type Flags = Flags;
    type Error = RecError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (x0, y0, x1, y1) = ctx.flags.crop;
        let w = x1.saturating_sub(x0).max(2);
        let h = y1.saturating_sub(y0).max(2);

        let bitrate = if w * h > 1920 * 1080 { 16_000_000 } else { 8_000_000 };
        let video = VideoSettingsBuilder::new(w, h)
            .sub_type(VideoSettingsSubType::H264)
            .frame_rate(30)
            .bitrate(bitrate);

        let audio = match ctx.flags.audio {
            Some((rate, channels)) => AudioSettingsBuilder::new()
                .sample_rate(rate)
                .channel_count(channels as u32)
                .bit_per_sample(16)
                .bitrate(128_000),
            None => AudioSettingsBuilder::new().disabled(true),
        };

        let encoder = VideoEncoder::new(
            video,
            audio,
            ContainerSettingsBuilder::default(),
            &ctx.flags.out,
        )?;

        Ok(Self {
            encoder: Some(encoder),
            crop: ctx.flags.crop,
            buf: Vec::new(),
            started: ctx.flags.started,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let ts = frame.timestamp()?.Duration;
        let (x0, y0, x1, y1) = self.crop;

        let mut fb = frame.buffer_crop(x0, y0, x1, y1)?;
        let w = fb.width() as usize;
        let h = fb.height() as usize;
        let pitch = fb.row_pitch() as usize;
        let raw = fb.as_raw_buffer();

        let need = w * h * 4;
        if self.buf.len() != need {
            self.buf.resize(need, 0);
        }
        // Media Foundation 的 buffer 路径要 bottom-up：顺便在这里翻转。
        for y in 0..h {
            let src = y * pitch;
            let dst = (h - 1 - y) * w * 4;
            self.buf[dst..dst + w * 4].copy_from_slice(&raw[src..src + w * 4]);
        }

        self.started.store(true, Ordering::Relaxed);
        if let Some(enc) = self.encoder.as_mut() {
            enc.send_frame_buffer(&self.buf, ts)?;
        }
        Ok(())
    }
}

pub fn start(
    monitor_index: usize,
    crop: (u32, u32, u32, u32),
    out: String,
    audio: Option<(u32, u16)>,
    started: Arc<AtomicBool>,
) -> Result<Control, RecError> {
    let monitor = windows_capture::monitor::Monitor::from_index(monitor_index)?;
    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(Duration::from_millis(33)),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        Flags {
            crop,
            out,
            audio,
            started,
        },
    );
    Ok(Recorder::start_free_threaded(settings)?)
}
