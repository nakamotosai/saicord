# CHANGELOG

## v0.2.0 — 2026-11-02

- 音频改为**系统声音**：WASAPI loopback 抓默认播放设备，不再采麦克风，不做混音
- 移除 cpal 依赖，二进制 551 KB → 498 KB
- 工具条按钮改为「系统声 开/关」，无播放设备时显示「无音频设备」
- 修 `IMMDevice::Activate` 缺 feature 导致的编译失败（`Win32_System_Variant`）


## v0.1.0 — 2026-11-02

首个版本。

- 框选任意矩形区域录制，录制期间选区边框常亮（画在选区外侧，不进画面）
- Windows Graphics Capture 抓屏，D3D11 侧裁切，只回读选区
- Media Foundation 硬件 H.264 + AAC，输出 MP4
- cpal 采集默认麦克风，音画同步起算
- F9 / 点击计时条停止，自动保存到 `桌面\录屏_YYYYMMDD_HHMMSS.mp4`
- 按显示器 DPI 缩放整套界面（200% 下按钮不变形）
- Esc / 右键取消选区
