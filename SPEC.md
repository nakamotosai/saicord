# SPEC · M3 系统声音

## 目标

录屏时把**扬声器里正在放的声音**一起录下来。不采麦克风，不做混音。

## 范围

1. `src/audio.rs` 改为 WASAPI loopback（`AUDCLNT_STREAMFLAGS_LOOPBACK`，默认渲染端点）
2. 采样率/声道数以设备 mix format 为准（实测 48 kHz / 2ch / float32）
3. float32 → 16-bit PCM 后送 Media Foundation 编码器
4. 工具条文案：`系统声 开 / 关`，无播放设备时显示 `无音频设备`
5. 移除 cpal 依赖

## 非目标

- 麦克风采集（未来若做，也只做二选一，不混音）
- 音量调节、降噪、回声消除
- 应用级分流（只录某个进程的声音）

## 验收条件

| # | 条件 | 判定方式 |
|---|---|---|
| B1 | 播放 440 Hz 音源录制后，音轨峰值频率 = 440 Hz | FFT 扫描 200–800 Hz |
| B2 | 有音源时 max_volume 明显高于静音（> -40 dB） | `ffmpeg volumedetect` |
| B3 | 音轨为 AAC 48 kHz 立体声 | `ffprobe` |
| B4 | 无播放设备时不崩溃，只录画面 | 断设备跑一次 |
| B5 | 二进制体积不增 | `ls -l`（551 KB → 498 KB） |
