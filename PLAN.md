# PLAN · M3 系统声音

## 步骤

1. **重写 `src/audio.rs`**
   - `CoCreateInstance(CLSID_MMDeviceEnumerator)` → `GetDefaultAudioEndpoint(eRender, eConsole)`
   - `IMMDevice::Activate::<IAudioClient>` → `GetMixFormat` → `Initialize(SHARED, LOOPBACK, 1s, 0, fmt)`
   - `GetService::<IAudioCaptureClient>` → `Start` → 轮询 `GetNextPacketSize` / `GetBuffer` / `ReleaseBuffer`
   - float32 → i16，`AUDCLNT_BUFFERFLAGS_SILENT` 时补零
2. **Cargo 依赖调整**
   - 删 `cpal`
   - 加 feature：`Win32_Media_Audio`、`Win32_System_Com`、`Win32_System_Com_StructuredStorage`、`Win32_System_Variant`
3. **UI 文案**：麦克风 → 系统声；`mic_*` 字段改名 `audio_*`
4. **验证**
   - 用 `System.Media.SoundPlayer` 循环播放 440 Hz WAV
   - 录 3 秒 → `volumedetect` 看电平 → 解析音轨做 FFT 找峰值
5. **收口**：CHANGELOG / README / HANDOFF / TRACK_TREE / progress / CLOSEOUT，tag v0.2.0

## 验证方式

`ffmpeg` 导出单声道 WAV → 8192 点 DFT 扫描 200–800 Hz → 峰值必须落在 440 Hz。
