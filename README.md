# saicord

**框选 → 开始 → 自动出片到桌面。** Windows 上的极简区域录屏，Rust 原生实现。

没有编辑器、没有设备选择、没有设置页、没有开机自启。双击就跑，F9 停，视频已经在桌面上了。

```
双击 saicord.exe
  └─ 拖出要录的矩形
       └─ 点「● 开始」
            └─ F9 停止
                 └─ 桌面\录屏_20261102_104139.mp4
```

## 为什么又一个录屏工具

因为现成的太重。同类 Electron 方案常驻 6 个进程、674 MB 内存；saicord 是单个 551 KB 的可执行文件，空闲 48 MB，录制中约 150 MB。功能砍到只剩一条链路：**抓屏 → 编码 → 落盘**。

## 功能

- 拖拽框选任意矩形区域，实时显示尺寸
- 录制期间**选区边框常亮**（画在选区外侧，不会进画面）
- 系统声音（扬声器 loopback）开关，音画同步起算
- F9 或点计时条停止，自动保存到桌面
- 按显示器 DPI 缩放整套界面（200% 下按钮不会变小）
- Esc / 右键随时取消

## 技术栈

| 环节 | 实现 |
|---|---|
| 抓屏 | Windows Graphics Capture（D3D11 侧 `CopySubresourceRegion` 裁切，只回读选区） |
| 编码 | Media Foundation 硬件 H.264 + AAC，MP4 封装 |
| 音频 | WASAPI loopback 采默认播放设备，float32→16-bit PCM 直接送编码器 |
| 选区遮罩 | 分层窗口 + per-pixel alpha（`UpdateLayeredWindow`） |
| 界面 | 原生 Win32，无 GUI 框架 |

零第三方运行时依赖，不打包 ffmpeg，不需要 Node。

## 构建

```bash
cargo build --release
# target/release/saicord.exe
```

需要 Rust 1.85+（依赖 `windows-capture` 使用 edition 2024）和 MSVC 工具链。

## 已知限制

- 只覆盖光标所在的显示器；跨屏框选不支持
- 选区最小 32×32 逻辑像素
- 只录系统声音（扬声器），不录麦克风；两者不做混音
- 停止只认 F9 / 点击计时条；无托盘常驻

## Roadmap

- [ ] 麦克风采集（与系统声二选一，不混音）
- [ ] 多显示器 / 跨屏选区
- [ ] 可配置热键与保存目录
- [ ] 录制后托盘提示与「打开所在文件夹」

## English

Minimal region screen recorder for Windows. Drag a rectangle, press start, press F9, get an MP4 with system audio on your Desktop. Built in Rust on Windows Graphics Capture + Media Foundation — no Electron, no ffmpeg, no editor UI.

## 许可

MIT
